// QuectoClaw — Discord channel implementation using serenity

#[cfg(feature = "discord")]
mod implementation {
    use crate::bus::{MessageBus, OutboundMessage};
    use crate::channel::{BaseChannel, Channel};
    use async_trait::async_trait;
    use serenity::all::{GatewayIntents, Message, Ready};
    use serenity::prelude::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    struct Handler {
        base: Arc<BaseChannel>,
    }

    #[async_trait]
    impl EventHandler for Handler {
        async fn message(&self, ctx: Context, msg: Message) {
            if msg.author.bot {
                return;
            }

            let sender_id = format!("{}|{}", msg.author.id, msg.author.name);
            let chat_id = msg.channel_id.to_string();

            self.base
                .handle_message(&sender_id, &chat_id, &msg.content, vec![], HashMap::new())
                .await;
        }

        async fn ready(&self, _: Context, ready: Ready) {
            tracing::info!("Discord bot {} is connected!", ready.user.name);
        }
    }

    pub struct DiscordChannel {
        base: BaseChannel,
        token: String,
        running: Arc<Mutex<bool>>,
        client: Arc<Mutex<Option<Client>>>,
    }

    impl DiscordChannel {
        pub fn new(token: &str, allow_list: Vec<String>, bus: Arc<MessageBus>) -> Self {
            Self {
                base: BaseChannel::new("discord", allow_list, bus),
                token: token.to_string(),
                running: Arc::new(Mutex::new(false)),
                client: Arc::new(Mutex::new(None)),
            }
        }
    }

    #[async_trait]
    impl Channel for DiscordChannel {
        fn name(&self) -> &str {
            self.base.name()
        }

        async fn start(&self) -> anyhow::Result<()> {
            let mut running = self.running.lock().await;
            if *running {
                return Ok(());
            }

            *running = true;
            let token = self.token.clone();
            let base = Arc::new(BaseChannel::new(
                self.base.name(),
                self.base.allow_list().to_vec(),
                self.base.bus().clone(),
            ));

            tracing::info!(channel = %self.name(), "Starting Discord channel");

            let intents = GatewayIntents::GUILD_MESSAGES
                | GatewayIntents::DIRECT_MESSAGES
                | GatewayIntents::MESSAGE_CONTENT;

            let handler = Handler { base };
            let mut client = Client::builder(&token, intents)
                .event_handler(handler)
                .await?;

            let running_clone = self.running.clone();
            let client_ptr = self.client.clone();

            // We can't easily "stop" serenity client once it starts with .start()
            // but we can spawn it.
            tokio::spawn(async move {
                if let Err(why) = client.start().await {
                    tracing::error!("Discord client error: {:?}", why);
                }
                let mut r = running_clone.lock().await;
                *r = false;
            });

            Ok(())
        }

        async fn stop(&self) -> anyhow::Result<()> {
            let mut running = self.running.lock().await;
            *running = false;
            // Serenity client doesn't have a simple .stop(), usually you drop it or use a shard manager shutdown.
            Ok(())
        }

        async fn send(&self, msg: OutboundMessage) -> anyhow::Result<()> {
            let token = self.token.clone();
            let http = serenity::http::Http::new(&token);
            let channel_id: u64 = msg.chat_id.parse()?;

            let map = serde_json::json!({
                "content": msg.content,
            });

            http.send_message(channel_id.into(), &map, vec![]).await?;

            Ok(())
        }

        fn is_running(&self) -> bool {
            true
        }
    }
}

#[cfg(feature = "discord")]
pub use implementation::DiscordChannel;

#[cfg(not(feature = "discord"))]
pub struct DiscordChannel;

#[cfg(not(feature = "discord"))]
impl DiscordChannel {
    pub fn new(
        _token: &str,
        _allow_list: Vec<String>,
        _bus: std::sync::Arc<crate::bus::MessageBus>,
    ) -> Self {
        Self
    }
}

#[cfg(not(feature = "discord"))]
#[async_trait::async_trait]
impl crate::channel::Channel for DiscordChannel {
    fn name(&self) -> &str {
        "discord"
    }
    async fn start(&self) -> anyhow::Result<()> {
        anyhow::bail!("Discord feature not enabled")
    }
    async fn stop(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn send(&self, _msg: crate::bus::OutboundMessage) -> anyhow::Result<()> {
        anyhow::bail!("Discord feature not enabled")
    }
    fn is_running(&self) -> bool {
        false
    }
}
