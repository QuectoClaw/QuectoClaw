// QuectoClaw — Local Vector Database (RAG)
//
// Simple in-memory vector store with TF-IDF-based embedding and cosine similarity.
// Provides fast, dependency-free document retrieval for RAG-powered context.
// Supports file-backed JSON persistence.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Maximum embedding dimension (vocabulary size limit for TF-IDF).
const MAX_DIM: usize = 4096;

/// A single stored document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    pub text: String,
    pub metadata: HashMap<String, String>,
    embedding: Vec<f32>,
}

/// Search result with similarity score.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub id: String,
    pub text: String,
    pub metadata: HashMap<String, String>,
    pub score: f64,
}

/// In-memory vector store with TF-IDF embeddings and cosine similarity search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorStore {
    documents: Vec<Document>,
    /// Global IDF vocabulary: word -> index
    vocabulary: HashMap<String, usize>,
    /// Document frequency for each vocabulary term
    doc_freq: HashMap<String, usize>,
    next_vocab_idx: usize,
}

impl VectorStore {
    pub fn new() -> Self {
        Self {
            documents: Vec::new(),
            vocabulary: HashMap::new(),
            doc_freq: HashMap::new(),
            next_vocab_idx: 0,
        }
    }

    /// Add a document to the store. The text is automatically embedded.
    pub fn add_document(&mut self, id: &str, text: &str, metadata: HashMap<String, String>) {
        // Remove existing document with same ID
        self.documents.retain(|d| d.id != id);

        // Tokenize and update vocabulary
        let tokens = tokenize(text);
        let unique_tokens: HashSet<&str> = tokens.iter().map(|s| s.as_str()).collect();

        for token in &unique_tokens {
            if !self.vocabulary.contains_key(*token) && self.next_vocab_idx < MAX_DIM {
                self.vocabulary
                    .insert(token.to_string(), self.next_vocab_idx);
                self.next_vocab_idx += 1;
            }
            *self.doc_freq.entry(token.to_string()).or_insert(0) += 1;
        }

        // Compute TF-IDF embedding
        let embedding = self.compute_embedding(&tokens);

        self.documents.push(Document {
            id: id.to_string(),
            text: text.to_string(),
            metadata,
            embedding,
        });
    }

    /// Search for the top-k most similar documents to the query.
    pub fn search(&self, query: &str, top_k: usize) -> Vec<SearchResult> {
        if self.documents.is_empty() {
            return Vec::new();
        }

        let tokens = tokenize(query);
        let query_embedding = self.compute_embedding(&tokens);

        let mut scored: Vec<(usize, f64)> = self
            .documents
            .iter()
            .enumerate()
            .map(|(i, doc)| (i, cosine_similarity(&query_embedding, &doc.embedding)))
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        scored
            .into_iter()
            .take(top_k)
            .filter(|(_, score)| *score > 0.0)
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

    /// Compute TF-IDF embedding for a token list.
    fn compute_embedding(&self, tokens: &[String]) -> Vec<f32> {
        let total_docs = self.documents.len().max(1) as f32;
        let mut embedding = vec![0.0f32; self.next_vocab_idx.min(MAX_DIM)];

        if tokens.is_empty() || embedding.is_empty() {
            return embedding;
        }

        // Count term frequencies
        let mut tf: HashMap<&str, f32> = HashMap::new();
        for token in tokens {
            *tf.entry(token.as_str()).or_insert(0.0) += 1.0;
        }
        let max_tf = tf.values().copied().fold(0.0f32, f32::max).max(1.0);

        // Compute TF-IDF for each term
        for (term, freq) in &tf {
            if let Some(&idx) = self.vocabulary.get(*term) {
                if idx < embedding.len() {
                    let normalized_tf = *freq / max_tf;
                    let df = self.doc_freq.get(*term).copied().unwrap_or(1) as f32;
                    let idf = (total_docs / df).ln() + 1.0;
                    embedding[idx] = normalized_tf * idf;
                }
            }
        }

        embedding
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

/// Tokenize text into lowercase words, filtering short tokens.
fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|s| s.len() >= 2)
        .map(String::from)
        .collect()
}

/// Cosine similarity between two vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
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
        store.add_document("1", "Rust programming language systems", HashMap::new());
        store.add_document(
            "2",
            "Python scripting language data science",
            HashMap::new(),
        );
        store.add_document("3", "cooking recipes pasta italian", HashMap::new());

        let results = store.search("Rust programming", 2);
        assert!(!results.is_empty());
        assert_eq!(results[0].id, "1");
    }

    #[test]
    fn test_empty_store() {
        let store = VectorStore::new();
        let results = store.search("anything", 5);
        assert!(results.is_empty());
        assert!(store.is_empty());
    }

    #[test]
    fn test_duplicate_id_replaces() {
        let mut store = VectorStore::new();
        store.add_document("1", "first version", HashMap::new());
        store.add_document("1", "second version", HashMap::new());
        assert_eq!(store.len(), 1);
        assert_eq!(store.documents[0].text, "second version");
    }

    #[test]
    fn test_chunk_text() {
        let text = "line 1\nline 2\nline 3\nline 4\nline 5";
        let chunks = chunk_text(text, 15);
        assert!(chunks.len() >= 2);
        for chunk in &chunks {
            assert!(chunk.len() <= 15 || !chunk.contains('\n'));
        }
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let v = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&v, &v);
        assert!((sim - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 0.0001);
    }

    #[test]
    fn test_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test_store.json");

        let mut store = VectorStore::new();
        store.add_document("doc1", "hello world", HashMap::new());
        store.save(&path).unwrap();

        let loaded = VectorStore::load(&path).unwrap();
        assert_eq!(loaded.len(), 1);
        let results = loaded.search("hello", 1);
        assert!(!results.is_empty());
    }
}
