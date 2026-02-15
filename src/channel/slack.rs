// QuectoClaw — Slack channel implementation (Web API for sending, stub for receiving)

use crate::bus::{MessageBus, OutboundMessage};
use crate::channel::{BaseChannel, Channel};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;

use std::sync::Arc;
use tokio::sync::Mutex;

pub struct SlackChannel {
    base: BaseChannel,
    bot_token: String,
    _app_token: String,
    client: Client,
    running: Arc<Mutex<bool>>,
}

impl SlackChannel {
    pub fn new(
        bot_token: &str,
        app_token: &str,
        allow_list: Vec<String>,
        bus: Arc<MessageBus>,
    ) -> Self {
        Self {
            base: BaseChannel::new("slack", allow_list, bus),
            bot_token: bot_token.to_string(),
            _app_token: app_token.to_string(),
            client: Client::new(),
            running: Arc::new(Mutex::new(false)),
        }
    }
}

#[async_trait]
impl Channel for SlackChannel {
    fn name(&self) -> &str {
        self.base.name()
    }

    async fn start(&self) -> anyhow::Result<()> {
        let mut running = self.running.lock().await;
        if *running {
            return Ok(());
        }

        *running = true;
        tracing::info!(channel = %self.name(), "Starting Slack channel (Web API mode)");

        // In a real implementation, we would start a Socket Mode client here using the app_token.
        // For now, we'll just log that it's "running" for outbound messages.

        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        let mut running = self.running.lock().await;
        *running = false;
        Ok(())
    }

    async fn send(&self, msg: OutboundMessage) -> anyhow::Result<()> {
        let url = "https://slack.com/api/chat.postMessage";

        let body = json!({
            "channel": msg.chat_id,
            "text": msg.content,
        });

        let resp = self
            .client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.bot_token))
            .header("Content-Type", "application/json; charset=utf-8")
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        let resp_body: serde_json::Value = resp.json().await?;

        if !status.is_success() || !resp_body["ok"].as_bool().unwrap_or(false) {
            anyhow::bail!(
                "Slack API error: {}",
                resp_body["error"].as_str().unwrap_or("unknown error")
            );
        }

        Ok(())
    }

    fn is_running(&self) -> bool {
        true
    }
}
