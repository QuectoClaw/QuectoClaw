// QuectoClaw — Telegram channel implementation using teloxide

#[cfg(feature = "telegram")]
mod implementation {
    use crate::bus::{MessageBus, OutboundMessage};
    use crate::channel::{BaseChannel, Channel};
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Arc;
    use teloxide::prelude::*;
    use teloxide::types::ParseMode;
    use tokio::sync::Mutex;

    pub struct TelegramChannel {
        base: BaseChannel,
        bot: Bot,
        running: Arc<Mutex<bool>>,
    }

    impl TelegramChannel {
        pub fn new(token: &str, allow_list: Vec<String>, bus: Arc<MessageBus>) -> Self {
            let bot = Bot::new(token);
            Self {
                base: BaseChannel::new("telegram", allow_list, bus),
                bot,
                running: Arc::new(Mutex::new(false)),
            }
        }
    }

    #[async_trait]
    impl Channel for TelegramChannel {
        fn name(&self) -> &str {
            self.base.name()
        }

        async fn start(&self) -> anyhow::Result<()> {
            let mut running = self.running.lock().await;
            if *running {
                return Ok(());
            }

            *running = true;
            let bot = self.bot.clone();
            let base = Arc::new(BaseChannel::new(
                self.base.name(),
                self.base.allow_list().to_vec(),
                self.base.bus().clone(),
            ));

            tracing::info!(channel = %self.name(), "Starting Telegram channel");

            // Clone for the background task
            let running_clone = self.running.clone();

            tokio::spawn(async move {
                let handler = Update::filter_message().endpoint(
                    |bot: Bot, base: Arc<BaseChannel>, msg: Message| async move {
                        if let Some(text) = msg.text() {
                            let sender_id = if let Some(user) = msg.from() {
                                if let Some(username) = &user.username {
                                    format!("{}|{}", user.id, username)
                                } else {
                                    user.id.to_string()
                                }
                            } else {
                                msg.chat.id.to_string()
                            };

                            let chat_id = msg.chat.id.to_string();

                            base.handle_message(&sender_id, &chat_id, text, vec![], HashMap::new())
                                .await;
                        }
                        respond(())
                    },
                );

                Dispatcher::builder(bot, handler)
                    .dependencies(dptree::deps![base])
                    .enable_ctrlc_handler()
                    .build()
                    .dispatch()
                    .await;

                let mut r = running_clone.lock().await;
                *r = false;
                tracing::info!("Telegram dispatcher stopped");
            });

            Ok(())
        }

        async fn stop(&self) -> anyhow::Result<()> {
            let mut running = self.running.lock().await;
            *running = false;
            // teloxide handles shutdown via ctrl-c or you can stop the task
            // For now, we just update the flag.
            Ok(())
        }

        async fn send(&self, msg: OutboundMessage) -> anyhow::Result<()> {
            let chat_id: i64 = msg.chat_id.parse()?;

            // Format content — convert markdown-ish to something Telegram likes or just send as plain text
            // For now, we'll use MarkdownV2 if possible, or just raw text.
            self.bot
                .send_message(ChatId(chat_id), &msg.content)
                .parse_mode(ParseMode::MarkdownV2) // This might fail if the LLM output isn't perfect MarkdownV2
                .send()
                .await
                .or_else(|_| {
                    // Fallback to plain text if MarkdownV2 fails
                    self.bot.send_message(ChatId(chat_id), &msg.content).send()
                })
                .await?;

            Ok(())
        }

        fn is_running(&self) -> bool {
            // We can't easily check the lock here synchronously if start() is async
            // but for simplicity we'll just track it.
            true
        }
    }
}

#[cfg(feature = "telegram")]
pub use implementation::TelegramChannel;

#[cfg(not(feature = "telegram"))]
pub struct TelegramChannel;

#[cfg(not(feature = "telegram"))]
impl TelegramChannel {
    pub fn new(
        _token: &str,
        _allow_list: Vec<String>,
        _bus: std::sync::Arc<crate::bus::MessageBus>,
    ) -> Self {
        Self
    }
}

#[cfg(not(feature = "telegram"))]
#[async_trait::async_trait]
impl crate::channel::Channel for TelegramChannel {
    fn name(&self) -> &str {
        "telegram"
    }
    async fn start(&self) -> anyhow::Result<()> {
        anyhow::bail!("Telegram feature not enabled")
    }
    async fn stop(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn send(&self, _msg: crate::bus::OutboundMessage) -> anyhow::Result<()> {
        anyhow::bail!("Telegram feature not enabled")
    }
    fn is_running(&self) -> bool {
        false
    }
}
