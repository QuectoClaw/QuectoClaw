// QuectoClaw — Local Vector Database (RAG)
//
// In-memory vector store with dense embedding search and cosine similarity.
// Provides semantic document retrieval for RAG-powered context.
// Supports file-backed JSON persistence.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// A single stored document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    pub text: String,
    pub metadata: HashMap<String, String>,
    pub embedding: Vec<f32>,
}

/// Search result with similarity score.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub id: String,
    pub text: String,
    pub metadata: HashMap<String, String>,
    pub score: f64,
}

/// In-memory vector store with semantic embeddings and cosine similarity search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorStore {
    documents: Vec<Document>,
}

impl VectorStore {
    pub fn new() -> Self {
        Self {
            documents: Vec::new(),
        }
    }

    /// Add a document with a pre-computed embedding.
    pub fn add_document_with_embedding(
        &mut self,
        id: &str,
        text: &str,
        metadata: HashMap<String, String>,
        embedding: Vec<f32>,
    ) {
        self.documents.retain(|d| d.id != id);
        self.documents.push(Document {
            id: id.to_string(),
            text: text.to_string(),
            metadata,
            embedding,
        });
    }

    /// Search for documents similar to a provided embedding.
    pub fn search_by_embedding(&self, embedding: &[f32], top_k: usize) -> Vec<SearchResult> {
        if self.documents.is_empty() {
            return Vec::new();
        }

        let mut scored: Vec<(usize, f64)> = self
            .documents
            .iter()
            .enumerate()
            .map(|(i, doc)| (i, cosine_similarity(embedding, &doc.embedding)))
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        scored
            .into_iter()
            .take(top_k)
            .filter(|(_, score)| *score > 0.1) // Minimum threshold
            .map(|(i, score)| {
                let doc = &self.documents[i];
                SearchResult {
                    id: doc.id.clone(),
                    text: doc.text.clone(),
                    metadata: doc.metadata.clone(),
                    score,
                }
            })
            .collect()
    }

    /// Number of documents in the store.
    pub fn len(&self) -> usize {
        self.documents.len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }

    /// Save the store to a JSON file.
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        let json = serde_json::to_string(self)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load the store from a JSON file.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let json = std::fs::read_to_string(path)?;
        let store: Self = serde_json::from_str(&json)?;
        Ok(store)
    }
}

impl Default for VectorStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Chunk text into smaller segments for indexing.
pub fn chunk_text(text: &str, max_chunk_size: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();

    for line in text.lines() {
        if current.len() + line.len() + 1 > max_chunk_size && !current.is_empty() {
            chunks.push(current.clone());
            current.clear();
        }
        if !current.is_empty() {
            current.push('\n');
        }
        current.push_str(line);
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

/// Cosine similarity between two vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    let len = a.len().min(b.len());
    if len == 0 {
        return 0.0;
    }

    let mut dot = 0.0f64;
    let mut mag_a = 0.0f64;
    let mut mag_b = 0.0f64;

    for i in 0..len {
        let va = a[i] as f64;
        let vb = b[i] as f64;
        dot += va * vb;
        mag_a += va * va;
        mag_b += vb * vb;
    }

    let denom = mag_a.sqrt() * mag_b.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_and_search() {
        let mut store = VectorStore::new();
        let metadata = HashMap::new();

        store.add_document_with_embedding("1", "doc 1", metadata.clone(), vec![1.0, 0.0]);
        store.add_document_with_embedding("2", "doc 2", metadata.clone(), vec![0.0, 1.0]);

        let results = store.search_by_embedding(&[1.0, 0.1], 1);
        assert_eq!(results[0].id, "1");
    }

    #[test]
    fn test_cosine_similarity() {
        let sim = cosine_similarity(&[1.0, 0.0], &[1.0, 0.0]);
        assert!((sim - 1.0).abs() < 0.001);

        let sim = cosine_similarity(&[1.0, 0.0], &[0.0, 1.0]);
        assert!(sim.abs() < 0.001);
    }
}
