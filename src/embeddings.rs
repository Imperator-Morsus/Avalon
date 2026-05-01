use reqwest::Client;
use std::time::Duration;

/// Embedding service that calls Ollama's /api/embeddings endpoint.
/// Default model: nomic-embed-text:v1.5 (768-dim f32 vectors).
#[derive(Clone)]
pub struct EmbeddingService {
    client: Client,
    base_url: String,
    model: String,
    dim: usize,
}

impl EmbeddingService {
    pub fn new() -> Result<Self, String> {
        let base_url = std::env::var("AVALON_OLLAMA_BASE")
            .unwrap_or_else(|_| "http://localhost:11434".to_string());
        let model = std::env::var("AVALON_EMBED_MODEL")
            .unwrap_or_else(|_| "nomic-embed-text:v1.5".to_string());
        let dim = std::env::var("AVALON_EMBED_DIM")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(768);

        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

        Ok(EmbeddingService {
            client,
            base_url,
            model,
            dim,
        })
    }

    pub fn with_model(base_url: &str, model: &str, dim: usize) -> Result<Self, String> {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

        Ok(EmbeddingService {
            client,
            base_url: base_url.to_string(),
            model: model.to_string(),
            dim,
        })
    }

    /// Generate a single embedding vector for the given text.
    pub async fn generate(&self, text: &str) -> Result<Vec<f32>, String> {
        let url = format!("{}/api/embeddings", self.base_url);
        let body = serde_json::json!({
            "model": self.model,
            "prompt": text,
        });

        let resp = self.client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Embedding request failed: {}", e))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Ollama embeddings returned {}: {}", status, text));
        }

        let json: serde_json::Value = resp.json().await
            .map_err(|e| format!("Failed to parse embedding response: {}", e))?;

        let embedding = json.get("embedding")
            .and_then(|v| v.as_array())
            .ok_or_else(|| "Missing embedding array in response".to_string())?;

        let vec: Vec<f32> = embedding.iter()
            .filter_map(|v| v.as_f64().map(|f| f as f32))
            .collect();

        if vec.len() != self.dim {
            return Err(format!(
                "Embedding dimension mismatch: expected {}, got {}",
                self.dim, vec.len()
            ));
        }

        Ok(vec)
    }

    /// Generate embeddings for a batch of texts.
    pub async fn generate_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            results.push(self.generate(text).await?);
        }
        Ok(results)
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn dim(&self) -> usize {
        self.dim
    }
}

/// Cosine similarity between two equal-length f32 vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f64;
    let mut norm_a = 0.0f64;
    let mut norm_b = 0.0f64;
    for i in 0..a.len() {
        let av = a[i] as f64;
        let bv = b[i] as f64;
        dot += av * bv;
        norm_a += av * av;
        norm_b += bv * bv;
    }
    let denom = (norm_a.sqrt() * norm_b.sqrt()).max(1e-10);
    (dot / denom).max(-1.0).min(1.0)
}

/// Convert a Vec<f32> to raw little-endian bytes for SQLite BLOB storage.
pub fn embedding_to_bytes(emb: &[f32]) -> Vec<u8> {
    emb.iter()
        .flat_map(|f| f.to_le_bytes().to_vec())
        .collect()
}

/// Convert raw little-endian bytes back to Vec<f32>.
pub fn bytes_to_embedding(bytes: &[u8]) -> Vec<f32> {
    bytes.chunks_exact(4)
        .map(|chunk| {
            let arr: [u8; 4] = chunk.try_into().unwrap_or([0; 4]);
            f32::from_le_bytes(arr)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0f32, 0.0, 0.0];
        let b = vec![1.0f32, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);

        let c = vec![0.0f32, 1.0, 0.0];
        assert!((cosine_similarity(&a, &c)).abs() < 1e-6);
    }

    #[test]
    fn test_embedding_roundtrip() {
        let emb = vec![1.5f32, 2.5, -0.5];
        let bytes = embedding_to_bytes(&emb);
        let recovered = bytes_to_embedding(&bytes);
        assert_eq!(emb, recovered);
    }
}
