// QuectoClaw — Channel trait and base channel

pub mod discord;
pub mod slack;
pub mod telegram;

use crate::bus::{InboundMessage, MessageBus, OutboundMessage};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

/// Channel is the interface that all chat channels must implement.
#[async_trait]
pub trait Channel: Send + Sync {
    fn name(&self) -> &str;
    async fn start(&self) -> anyhow::Result<()>;
    async fn stop(&self) -> anyhow::Result<()>;
    async fn send(&self, msg: OutboundMessage) -> anyhow::Result<()>;
    fn is_running(&self) -> bool;
}

/// BaseChannel provides common functionality for all channels.
pub struct BaseChannel {
    channel_name: String,
    allow_list: Vec<String>,
    bus: Arc<MessageBus>,
}

impl BaseChannel {
    pub fn new(name: &str, allow_list: Vec<String>, bus: Arc<MessageBus>) -> Self {
        Self {
            channel_name: name.to_string(),
            allow_list,
            bus,
        }
    }

    /// Check if a sender is allowed to use this channel.
    /// Empty allowlist = deny all (secure by default).
    /// Use `["*"]` to explicitly allow all senders.
    pub fn is_allowed(&self, sender_id: &str) -> bool {
        if self.allow_list.is_empty() {
            return false; // deny-all by default — secure posture
        }

        // Wildcard: explicit opt-in to allow everyone
        if self.allow_list.iter().any(|a| a == "*") {
            return true;
        }

        let (id_part, user_part) = if let Some(idx) = sender_id.find('|') {
            (&sender_id[..idx], Some(&sender_id[idx + 1..]))
        } else {
            (sender_id, None)
        };

        for allowed in &self.allow_list {
            let trimmed = allowed.trim_start_matches('@');
            if sender_id == allowed
                || sender_id == trimmed
                || id_part == allowed
                || id_part == trimmed
            {
                return true;
            }
            if let Some(user) = user_part {
                if user == allowed || user == trimmed {
                    return true;
                }
            }
        }

        false
    }

    /// Route an inbound message to the agent via the message bus.
    pub async fn handle_message(
        &self,
        sender_id: &str,
        chat_id: &str,
        content: &str,
        media: Vec<String>,
        metadata: HashMap<String, String>,
    ) {
        if !self.is_allowed(sender_id) {
            tracing::warn!(
                channel = %self.channel_name,
                sender = %sender_id,
                "Message blocked: sender not in allow list"
            );
            return;
        }

        let session_key = format!("{}:{}", self.channel_name, chat_id);

        self.bus
            .publish_inbound(InboundMessage {
                channel: self.channel_name.clone(),
                sender_id: sender_id.to_string(),
                chat_id: chat_id.to_string(),
                content: content.to_string(),
                media,
                session_key,
                metadata,
            })
            .await;
    }

    pub fn name(&self) -> &str {
        &self.channel_name
    }

    pub fn allow_list(&self) -> &[String] {
        &self.allow_list
    }

    pub fn bus(&self) -> &Arc<MessageBus> {
        &self.bus
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allow_list_empty_denies_all() {
        let bus = Arc::new(MessageBus::new());
        let ch = BaseChannel::new("test", vec![], bus);
        assert!(!ch.is_allowed("anyone"));
    }

    #[test]
    fn test_allow_list_wildcard_allows_all() {
        let bus = Arc::new(MessageBus::new());
        let ch = BaseChannel::new("test", vec!["*".into()], bus);
        assert!(ch.is_allowed("anyone"));
    }

    #[test]
    fn test_allow_list_exact() {
        let bus = Arc::new(MessageBus::new());
        let ch = BaseChannel::new("test", vec!["user123".into()], bus);
        assert!(ch.is_allowed("user123"));
        assert!(!ch.is_allowed("user456"));
    }

    #[test]
    fn test_allow_list_compound_id() {
        let bus = Arc::new(MessageBus::new());
        let ch = BaseChannel::new("test", vec!["123".into()], bus);
        assert!(ch.is_allowed("123|john"));
    }

    #[test]
    fn test_allow_list_at_prefix() {
        let bus = Arc::new(MessageBus::new());
        let ch = BaseChannel::new("test", vec!["@john".into()], bus);
        assert!(ch.is_allowed("john"));
    }
}
