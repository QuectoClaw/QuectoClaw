// QuectoClaw — Gateway service (runs all channels and agent loop)

use crate::agent::AgentLoop;
use crate::bus::{MessageBus, OutboundMessage};
use crate::channel::discord::DiscordChannel;
use crate::channel::slack::SlackChannel;
use crate::channel::telegram::TelegramChannel;
use crate::channel::Channel;
use crate::config::Config;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};

pub struct Gateway {
    config: Config,
    agent: Arc<AgentLoop>,
    bus: Arc<MessageBus>,
    channels: Vec<Arc<dyn Channel>>,
}

impl Gateway {
    pub fn new(config: Config, agent: Arc<AgentLoop>, bus: Arc<MessageBus>) -> Self {
        let mut channels: Vec<Arc<dyn Channel>> = Vec::new();

        // Initialize Telegram if enabled
        if config.channels.telegram.enabled {
            let ch = TelegramChannel::new(
                &config.channels.telegram.token,
                config.channels.telegram.allow_from.clone(),
                bus.clone(),
            );
            channels.push(Arc::new(ch));
        }

        // Initialize Discord if enabled
        if config.channels.discord.enabled {
            let ch = DiscordChannel::new(
                &config.channels.discord.token,
                config.channels.discord.allow_from.clone(),
                bus.clone(),
            );
            channels.push(Arc::new(ch));
        }

        // Initialize Slack if enabled
        if config.channels.slack.enabled {
            let ch = SlackChannel::new(
                &config.channels.slack.bot_token,
                &config.channels.slack.app_token,
                config.channels.slack.allow_from.clone(),
                bus.clone(),
            );
            channels.push(Arc::new(ch));
        }

        Self {
            config,
            agent,
            bus,
            channels,
        }
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        let mut set = tokio::task::JoinSet::new();

        tracing::info!("Gateway starting with {} channels", self.channels.len());

        // 1. Start all channels
        for channel in &self.channels {
            let ch = channel.clone();
            let name = ch.name().to_string();

            // Register a handler in the bus for this channel
            let (tx, mut rx) = mpsc::channel::<OutboundMessage>(100);
            self.bus.register_handler(&name, tx).await;

            // Spawn the channel's background task
            if let Err(e) = ch.start().await {
                tracing::error!(channel = %name, "Failed to start channel: {}", e);
                continue;
            }

            // Spawn a task to bridge bus outbound -> channel send
            let ch_inner = ch.clone();
            set.spawn(async move {
                while let Some(msg) = rx.recv().await {
                    if let Err(e) = ch_inner.send(msg).await {
                        tracing::error!(channel = %name, "Failed to send message: {}", e);
                    }
                }
                tracing::info!(channel = %name, "Outbound bridge stopped");
            });
        }

        // 2. Start the Agent Loop
        let agent = self.agent.clone();
        set.spawn(async move {
            if let Err(e) = agent.run().await {
                tracing::error!("Agent loop stopped with error: {}", e);
            }
        });

        // 3. Start Heartbeat Service
        if self.config.heartbeat.enabled {
            let agent = self.agent.clone();
            let bus = self.bus.clone();
            let interval_secs = self.config.heartbeat.interval;
            set.spawn(async move {
                heartbeat_service(agent, bus, interval_secs).await;
            });
        }

        // Wait for all tasks (though they should run indefinitely)
        while let Some(res) = set.join_next().await {
            match res {
                Ok(_) => tracing::info!("Task completed"),
                Err(e) => tracing::error!("Task failed: {}", e),
            }
        }

        Ok(())
    }
}

/// Heartbeat service: periodically checks HEARTBEAT.md and runs tasks.
async fn heartbeat_service(agent: Arc<AgentLoop>, _bus: Arc<MessageBus>, interval_mins: u64) {
    let mut ticker = interval(Duration::from_secs(interval_mins * 60));
    tracing::info!(interval_mins = interval_mins, "Heartbeat service started");

    loop {
        ticker.tick().await;
        tracing::debug!("Heartbeat tick");

        let workspace = agent.workspace();
        let path = std::path::Path::new(workspace).join("HEARTBEAT.md");

        if path.exists() {
            if let Ok(content) = tokio::fs::read_to_string(&path).await {
                let trimmed = content.trim();
                if !trimmed.is_empty() {
                    tracing::info!("Heartbeat: found tasks in HEARTBEAT.md");
                    // System-level one-shot processing (no context usually)
                    let session_key = "system:heartbeat";
                    let prompt = format!("Periodic check of HEARTBEAT.md. Current tasks:\n{}\n\nPerform any pending actions.", trimmed);

                    if let Err(e) = agent.process_direct(&prompt, session_key).await {
                        tracing::error!("Heartbeat processing failed: {}", e);
                    }
                }
            }
        }
    }
}
