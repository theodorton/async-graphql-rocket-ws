pub use async_graphql::http::WebSocketProtocols as Protocols;
use async_graphql::{http::WsMessage, *};
use futures::{SinkExt, StreamExt};
use rocket_ws::{Message, WebSocket, frame::CloseFrame};

fn filter_inbound(msg: Message) -> impl Future<Output = Option<Message>> {
    std::future::ready(match msg {
        Message::Text(_) | Message::Binary(_) => Some(msg),
        _ => None,
    })
}

fn map_outbound(msg: WsMessage) -> Message {
    match msg {
        WsMessage::Text(text) => Message::Text(text.clone()),
        WsMessage::Close(code, reason) => {
            let close_frame = Some(CloseFrame {
                code: code.into(),
                reason: reason.into(),
            });
            Message::Close(close_frame)
        }
    }
}

pub struct GraphQLSubscription<Query, Mutation, Subscription> {
    schema: async_graphql::Schema<Query, Mutation, Subscription>,
    protocol: Protocols,
}

impl<Query, Mutation, Subscription> GraphQLSubscription<Query, Mutation, Subscription>
where
    Query: ObjectType + 'static,
    Mutation: ObjectType + 'static,
    Subscription: SubscriptionType + 'static,
{
    pub fn new(
        schema: &async_graphql::Schema<Query, Mutation, Subscription>,
        protocol: Protocols,
    ) -> Self {
        Self {
            schema: schema.clone(),
            protocol,
        }
    }

    pub fn start(self, ws: WebSocket) -> rocket_ws::Channel<'static> {
        ws.channel(move |stream| {
            Box::pin(async move {
                let (mut sink, stream) = futures_util::StreamExt::split(stream);

                // See https://github.com/async-graphql/async-graphql/blob/082201ccd040726410ac1b8a6cf6dc4a1c5179ec/integrations/axum/src/subscription.rs#L265-L276
                let stream = stream
                    .take_while(|res| std::future::ready(res.is_ok()))
                    .map(Result::unwrap)
                    .filter_map(filter_inbound)
                    .map(Message::into_data);

                let mut ws =
                    async_graphql::http::WebSocket::new(self.schema, stream, self.protocol)
                        .map(map_outbound);
                while let Some(message) = ws.next().await {
                    sink.send(message).await.expect("Failed to send message");
                }
                eprintln!("WebSocket connection closed");
                Ok(())
            })
        })
    }
}
