use serde_json::json;
use crate::tools::{Tool, ToolContext};
use crate::vault::VaultService;
use std::sync::{Arc, Mutex};

// ═════════════════════════════════════════════════════════════════════════════
// vault_extract_concepts Tool
// Uses a local LLM to extract key concepts from a vault item and create
// concept nodes linked to the source item.
// ═════════════════════════════════════════════════════════════════════════════

pub struct VaultExtractConceptsTool {
    vault: Arc<Mutex<VaultService>>,
}

impl VaultExtractConceptsTool {
    pub fn new(vault: Arc<Mutex<VaultService>>) -> Self {
        Self { vault }
    }

    async fn call_ollama_json(
        prompt: &str,
    ) -> Result<serde_json::Value, String> {
        let base = std::env::var("AVALON_OLLAMA_BASE")
            .unwrap_or_else(|_| "http://localhost:11434".to_string());
        let model = std::env::var("AVALON_LIBRARIAN_MODEL")
            .unwrap_or_else(|_| "qwen2.5-coder:7b".to_string());

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| e.to_string())?;

        let body = json!({
            "model": model,
            "prompt": prompt,
            "stream": false,
            "format": "json",
            "options": {
                "temperature": 0.2,
                "num_predict": 512
            }
        });

        let resp = client.post(format!("{}/api/generate", base))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Ollama request failed: {}", e))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Ollama returned {}: {}", status, text));
        }

        let resp_json: serde_json::Value = resp.json().await
            .map_err(|e| format!("Failed to parse Ollama response: {}", e))?;

        let response_text = resp_json.get("response")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();

        if response_text.is_empty() {
            return Err("Ollama returned empty response".to_string());
        }

        // Try to parse as JSON
        serde_json::from_str(response_text)
            .or_else(|_| {
                // If pure JSON parse fails, try to extract JSON array from markdown
                let cleaned = response_text
                    .replace("```json", "")
                    .replace("```", "")
                    .trim()
                    .to_string();
                serde_json::from_str(&cleaned)
            })
            .map_err(|e| format!("Failed to parse concept JSON: {}. Raw: {}", e, response_text))
    }
}

#[async_trait::async_trait]
impl Tool for VaultExtractConceptsTool {
    fn name(&self) -> &str {
        "vault_extract_concepts"
    }

    fn description(&self) -> &str {
        "Extract key concepts from a vault item using AI. Creates concept nodes and links them. Input: {\"item_id\": 1}"
    }

    fn is_core(&self) -> bool {
        false
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        ctx: &ToolContext<'_>,
    ) -> Result<serde_json::Value, String> {
        let item_id = input.get("item_id")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| {
                let e = "Missing 'item_id' argument.".to_string();
                ctx.audit("vault_extract_concepts_error", json!({
                    "error": &e,
                    "actor": ctx.actor,
                    "session_id": ctx.session_id,
                }));
                e
            })?;

        let item = match self.vault.lock().unwrap().get(item_id) {
            Ok(Some(i)) => i,
            Ok(None) => {
                let e = "Item not found.".to_string();
                ctx.audit("vault_extract_concepts_error", json!({
                    "item_id": item_id,
                    "error": &e,
                    "actor": ctx.actor,
                    "session_id": ctx.session_id,
                }));
                return Err(e);
            }
            Err(e) => {
                let e = e.to_string();
                ctx.audit("vault_extract_concepts_error", json!({
                    "item_id": item_id,
                    "error": &e,
                    "actor": ctx.actor,
                    "session_id": ctx.session_id,
                }));
                return Err(e);
            }
        };

        let text = {
            let mut parts = Vec::new();
            if let Some(ref title) = item.title {
                parts.push(format!("Title: {}", title));
            }
            if let Some(ref desc) = item.description {
                parts.push(format!("Description: {}", desc));
            }
            let content_preview: String = item.content.chars().take(3000).collect();
            parts.push(format!("Content: {}", content_preview));
            parts.join("\n")
        };

        let prompt = format!(
            "Extract 5-10 key concepts from the following text. Return ONLY a JSON array of strings. Each concept should be a concise noun phrase (1-4 words).\n\n{}\n\nConcepts:",
            text
        );

        let concepts_json = match Self::call_ollama_json(&prompt).await {
            Ok(v) => v,
            Err(e) => {
                ctx.audit("vault_extract_concepts_error", json!({
                    "item_id": item_id,
                    "error": &e,
                    "actor": ctx.actor,
                    "session_id": ctx.session_id,
                }));
                return Err(e);
            }
        };
        let concepts: Vec<String> = match concepts_json.as_array() {
            Some(arr) => arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
                .filter(|s| !s.is_empty())
                .collect(),
            None => {
                let e = "Expected JSON array of concepts".to_string();
                ctx.audit("vault_extract_concepts_error", json!({
                    "item_id": item_id,
                    "error": &e,
                    "actor": ctx.actor,
                    "session_id": ctx.session_id,
                }));
                return Err(e);
            }
        };

        if concepts.is_empty() {
            ctx.audit("vault_extract_concepts", json!({
                "item_id": item_id,
                "concepts_found": 0,
                "actor": ctx.actor,
                "session_id": ctx.session_id,
            }));
            return Ok(json!({"ok": true, "item_id": item_id, "concepts_found": 0}));
        }

        let mut concept_ids = Vec::new();
        let now = chrono::Utc::now().to_rfc3339();

        for concept in &concepts {
            // Check if concept already exists
            let existing = self.vault.lock().unwrap().search(concept, 5)
                .map_err(|e| e.to_string())?
                .into_iter()
                .find(|i| i.content_type == "concept" && i.title.as_deref() == Some(concept));

            let concept_id = if let Some(existing) = existing {
                existing.id
            } else {
                self.vault.lock().unwrap().ingest_text(
                    &format!("concept://{}", concept),
                    Some(concept),
                    "",
                    "concept",
                    "Public",
                    None,
                ).map_err(|e| e.to_string())?
            };

            // Link concept to source item
            let _ = self.vault.lock().unwrap().link_items(
                item_id, concept_id, "teaches", 0.95, Some("AI-extracted concept")
            );
            concept_ids.push(json!({"id": concept_id, "concept": concept}));
        }

        ctx.audit("vault_extract_concepts", json!({
            "item_id": item_id,
            "concepts_found": concepts.len(),
            "actor": ctx.actor,
            "session_id": ctx.session_id,
        }));
        Ok(json!({
            "ok": true,
            "item_id": item_id,
            "concepts_found": concepts.len(),
            "concepts": concept_ids,
        }))
    }
}
