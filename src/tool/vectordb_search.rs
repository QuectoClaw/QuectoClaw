// QuectoClaw — Vector Search Tool
//
// Searches the local vector database for relevant documents.

use crate::tool::{Tool, ToolResult};
use crate::vectordb::VectorStore;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct VectorSearchTool {
    store: Arc<RwLock<VectorStore>>,
}

impl VectorSearchTool {
    pub fn new(store: Arc<RwLock<VectorStore>>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for VectorSearchTool {
    fn name(&self) -> &str {
        "vectordb_search"
    }

    fn description(&self) -> &str {
        "Search the local vector database for documents similar to a query. Returns the most relevant documents based on semantic similarity."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query"
                },
                "top_k": {
                    "type": "integer",
                    "description": "Number of results to return (default: 5)",
                    "default": 5
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, args: HashMap<String, Value>) -> ToolResult {
        let query = match args.get("query").and_then(|v| v.as_str()) {
            Some(q) => q,
            None => return ToolResult::error("Missing required parameter: query"),
        };

        let top_k = args.get("top_k").and_then(|v| v.as_u64()).unwrap_or(5) as usize;

        let store = self.store.read().await;
        let results = store.search(query, top_k);

        if results.is_empty() {
            return ToolResult::success("No matching documents found.");
        }

        let mut output = format!("Found {} results:\n\n", results.len());
        for (i, result) in results.iter().enumerate() {
            output.push_str(&format!(
                "─── Result {} (score: {:.4}) ───\n",
                i + 1,
                result.score
            ));
            output.push_str(&format!("ID: {}\n", result.id));
            if !result.metadata.is_empty() {
                for (k, v) in &result.metadata {
                    output.push_str(&format!("{}: {}\n", k, v));
                }
            }
            let text = if result.text.len() > 500 {
                format!("{}...", &result.text[..500])
            } else {
                result.text.clone()
            };
            output.push_str(&format!("{}\n\n", text));
        }

        ToolResult::success(output)
    }
}
