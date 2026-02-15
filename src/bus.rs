// QuectoClaw — Message bus (async channels for inter-component communication)

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

// ---------------------------------------------------------------------------
// Message types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboundMessage {
    pub channel: String,
    pub sender_id: String,
    pub chat_id: String,
    pub content: String,
    #[serde(default)]
    pub media: Vec<String>,
    pub session_key: String,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundMessage {
    pub channel: String,
    pub chat_id: String,
    pub content: String,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// Message bus
// ---------------------------------------------------------------------------

pub struct MessageBus {
    inbound_tx: mpsc::Sender<InboundMessage>,
    inbound_rx: Arc<tokio::sync::Mutex<mpsc::Receiver<InboundMessage>>>,
    outbound_tx: mpsc::Sender<OutboundMessage>,
    outbound_rx: Arc<tokio::sync::Mutex<mpsc::Receiver<OutboundMessage>>>,
    handlers: Arc<RwLock<HashMap<String, mpsc::Sender<OutboundMessage>>>>,
}

impl MessageBus {
    pub fn new() -> Self {
        let (inbound_tx, inbound_rx) = mpsc::channel(100);
        let (outbound_tx, outbound_rx) = mpsc::channel(100);

        Self {
            inbound_tx,
            inbound_rx: Arc::new(tokio::sync::Mutex::new(inbound_rx)),
            outbound_tx,
            outbound_rx: Arc::new(tokio::sync::Mutex::new(outbound_rx)),
            handlers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn publish_inbound(&self, msg: InboundMessage) {
        if let Err(e) = self.inbound_tx.send(msg).await {
            tracing::error!("Failed to publish inbound message: {}", e);
        }
    }

    pub async fn consume_inbound(&self) -> Option<InboundMessage> {
        self.inbound_rx.lock().await.recv().await
    }

    pub async fn publish_outbound(&self, msg: OutboundMessage) {
        // Route to channel-specific handler if registered
        let handlers = self.handlers.read().await;
        if let Some(handler) = handlers.get(&msg.channel) {
            if let Err(e) = handler.send(msg.clone()).await {
                tracing::error!(channel = %msg.channel, "Failed to send to handler: {}", e);
            }
        }

        // Also publish to global outbound
        if let Err(e) = self.outbound_tx.send(msg).await {
            tracing::error!("Failed to publish outbound message: {}", e);
        }
    }

    pub async fn subscribe_outbound(&self) -> Option<OutboundMessage> {
        self.outbound_rx.lock().await.recv().await
    }

    /// Register a channel-specific handler for outbound messages.
    pub async fn register_handler(&self, channel: &str, sender: mpsc::Sender<OutboundMessage>) {
        self.handlers.write().await.insert(channel.to_string(), sender);
    }

    /// Get the inbound sender (for channels to publish messages).
    pub fn inbound_sender(&self) -> mpsc::Sender<InboundMessage> {
        self.inbound_tx.clone()
    }
}

impl Default for MessageBus {
    fn default() -> Self {
        Self::new()
    }
}
