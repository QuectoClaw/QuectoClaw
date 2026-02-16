// QuectoClaw — Vector Index Tool
//
// Indexes text or file content into the local vector database.

use crate::tool::{Tool, ToolResult};
use crate::vectordb::{chunk_text, VectorStore};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct VectorIndexTool {
    store: Arc<RwLock<VectorStore>>,
    provider: Arc<dyn crate::provider::LLMProvider>,
    workspace: String,
}

impl VectorIndexTool {
    pub fn new(
        store: Arc<RwLock<VectorStore>>,
        provider: Arc<dyn crate::provider::LLMProvider>,
        workspace: String,
    ) -> Self {
        Self {
            store,
            provider,
            workspace,
        }
    }
}

#[async_trait]
impl Tool for VectorIndexTool {
    fn name(&self) -> &str {
        "vectordb_index"
    }

    fn description(&self) -> &str {
        "Index text or a file into the local vector database for later semantic search. Automatically chunks large documents."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "Text content to index (mutually exclusive with 'file')"
                },
                "file": {
                    "type": "string",
                    "description": "File path to read and index (mutually exclusive with 'text')"
                },
                "id": {
                    "type": "string",
                    "description": "Document ID (auto-generated from file path or content hash if not provided)"
                }
            }
        })
    }

    async fn execute(&self, args: HashMap<String, Value>) -> ToolResult {
        let text = args.get("text").and_then(|v| v.as_str());
        let file = args.get("file").and_then(|v| v.as_str());
        let id = args.get("id").and_then(|v| v.as_str());

        let (content, source_id) = match (text, file) {
            (Some(t), _) => {
                let doc_id = id.map(String::from).unwrap_or_else(|| {
                    format!("text-{:x}", {
                        let mut h: u64 = 0;
                        for b in t.bytes() {
                            h = h.wrapping_mul(31).wrapping_add(b as u64);
                        }
                        h
                    })
                });
                (t.to_string(), doc_id)
            }
            (None, Some(f)) => {
                let path = if f.starts_with('/') {
                    std::path::PathBuf::from(f)
                } else {
                    std::path::PathBuf::from(&self.workspace).join(f)
                };
                match std::fs::read_to_string(&path) {
                    Ok(content) => {
                        let doc_id = id
                            .map(String::from)
                            .unwrap_or_else(|| path.to_string_lossy().to_string());
                        (content, doc_id)
                    }
                    Err(e) => return ToolResult::error(format!("Failed to read file: {}", e)),
                }
            }
            (None, None) => {
                return ToolResult::error("Either 'text' or 'file' parameter is required.")
            }
        };

        // Chunk and index
        let chunks = chunk_text(&content, 1000);

        // Generate embeddings for all chunks
        let embeddings = match self.provider.embeddings(chunks.clone(), "").await {
            Ok(e) => e,
            Err(e) => return ToolResult::error(format!("Failed to generate embeddings: {}", e)),
        };

        let mut store = self.store.write().await;

        for (i, (chunk, emb)) in chunks.iter().zip(embeddings).enumerate() {
            let chunk_id = if chunks.len() == 1 {
                source_id.clone()
            } else {
                format!("{}#chunk-{}", source_id, i)
            };

            let mut metadata = HashMap::new();
            metadata.insert("source".to_string(), source_id.clone());
            if chunks.len() > 1 {
                metadata.insert("chunk".to_string(), format!("{}/{}", i + 1, chunks.len()));
            }

            store.add_document_with_embedding(&chunk_id, chunk, metadata, emb);
        }

        // Persist to workspace
        let persist_path = std::path::Path::new(&self.workspace).join("memory/vectordb.json");
        if let Err(e) = store.save(&persist_path) {
            tracing::warn!("Failed to persist vector store: {}", e);
        }

        ToolResult::success(format!(
            "Indexed {} chunk(s) with ID prefix '{}'. Vector store now has {} documents.",
            chunks.len(),
            source_id,
            store.len()
        ))
    }
}
