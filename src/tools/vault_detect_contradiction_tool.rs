use serde_json::json;
use crate::tools::{Tool, ToolContext};
use crate::vault::VaultService;
use std::sync::{Arc, Mutex};

// ═════════════════════════════════════════════════════════════════════════════
// vault_detect_contradiction Tool
// Compares a vault item with its older version using a local LLM.
// If a contradiction is detected with confidence >= 0.85, creates a
// contradiction relationship and notification.
// ═════════════════════════════════════════════════════════════════════════════

pub struct VaultDetectContradictionTool {
    vault: Arc<Mutex<VaultService>>,
}

impl VaultDetectContradictionTool {
    pub fn new(vault: Arc<Mutex<VaultService>>) -> Self {
        Self { vault }
    }

    async fn call_ollama_json(prompt: &str) -> Result<serde_json::Value, String> {
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
                "temperature": 0.1,
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

        serde_json::from_str(response_text)
            .or_else(|_| {
                let cleaned = response_text
                    .replace("```json", "")
                    .replace("```", "")
                    .trim()
                    .to_string();
                serde_json::from_str(&cleaned)
            })
            .map_err(|e| format!("Failed to parse contradiction JSON: {}. Raw: {}", e, response_text))
    }
}

#[async_trait::async_trait]
impl Tool for VaultDetectContradictionTool {
    fn name(&self) -> &str {
        "vault_detect_contradiction"
    }

    fn description(&self) -> &str {
        "Detect contradictions between a vault item and its older version. Input: {\"item_id\": 1}. Surfaces conflicts without declaring truth."
    }

    fn is_core(&self) -> bool {
        false
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        ctx: &ToolContext<'_>,
    ) -> Result<serde_json::Value, String> {
        let item_id = match input.get("item_id").and_then(|v| v.as_i64()) {
            Some(id) => id,
            None => {
                let e = "Missing 'item_id' argument.".to_string();
                ctx.audit("vault_detect_contradiction_error", json!({
                    "error": &e,
                    "actor": ctx.actor,
                    "session_id": ctx.session_id,
                }));
                return Err(e);
            }
        };

        let rels = match self.vault.lock().unwrap().get_related_items(item_id) {
            Ok(r) => r,
            Err(e) => {
                let e = e.to_string();
                ctx.audit("vault_detect_contradiction_error", json!({
                    "item_id": item_id,
                    "error": &e,
                    "actor": ctx.actor,
                    "session_id": ctx.session_id,
                }));
                return Err(e);
            }
        };

        let older_rel = rels.iter().find(|r| r.relation_type == "older_version");

        if older_rel.is_none() {
            ctx.audit("vault_detect_contradiction", json!({
                "item_id": item_id,
                "contradicts": false,
                "confidence": 0,
                "flagged": false,
                "reason": "No older version found",
                "actor": ctx.actor,
                "session_id": ctx.session_id,
            }));
            return Ok(json!({
                "ok": true,
                "item_id": item_id,
                "contradictions_found": 0,
                "message": "No older version found to compare against."
            }));
        }

        let older_id = older_rel.unwrap().target_id;

        let (newer_item, older_item) = {
            let vault = self.vault.lock().unwrap();
            let newer = match vault.get(item_id) {
                Ok(Some(i)) => i,
                Ok(None) => {
                    let e = "Item not found".to_string();
                    ctx.audit("vault_detect_contradiction_error", json!({
                        "item_id": item_id,
                        "error": &e,
                        "actor": ctx.actor,
                        "session_id": ctx.session_id,
                    }));
                    return Err(e);
                }
                Err(e) => {
                    let e = e.to_string();
                    ctx.audit("vault_detect_contradiction_error", json!({
                        "item_id": item_id,
                        "error": &e,
                        "actor": ctx.actor,
                        "session_id": ctx.session_id,
                    }));
                    return Err(e);
                }
            };
            let older = match vault.get(older_id) {
                Ok(Some(o)) => o,
                Ok(None) => {
                    let e = "Older item not found".to_string();
                    ctx.audit("vault_detect_contradiction_error", json!({
                        "item_id": item_id,
                        "older_id": older_id,
                        "error": &e,
                        "actor": ctx.actor,
                        "session_id": ctx.session_id,
                    }));
                    return Err(e);
                }
                Err(e) => {
                    let e = e.to_string();
                    ctx.audit("vault_detect_contradiction_error", json!({
                        "item_id": item_id,
                        "older_id": older_id,
                        "error": &e,
                        "actor": ctx.actor,
                        "session_id": ctx.session_id,
                    }));
                    return Err(e);
                }
            };
            (newer, older)
        };

        let prompt = format!(
            "Compare these two excerpts from different versions of the same document. Do they contain contradictory claims or information?\n\nReturn ONLY a JSON object with these exact keys:\n- \"contradicts\": boolean (true if they contradict)\n- \"reason\": string (one sentence explaining the conflict, or \"No contradiction detected\" if they agree)\n- \"confidence\": number 0.0-1.0 (your certainty)\n\nOLDER VERSION:\n{}\n\nNEWER VERSION:\n{}\n\nAnalysis:",
            older_item.content.chars().take(2000).collect::<String>(),
            newer_item.content.chars().take(2000).collect::<String>()
        );

        let result = match Self::call_ollama_json(&prompt).await {
            Ok(r) => r,
            Err(e) => {
                ctx.audit("vault_detect_contradiction_error", json!({
                    "item_id": item_id,
                    "older_id": older_id,
                    "error": &e,
                    "actor": ctx.actor,
                    "session_id": ctx.session_id,
                }));
                return Err(e);
            }
        };
        let contradicts = result.get("contradicts").and_then(|v| v.as_bool()).unwrap_or(false);
        let reason = result.get("reason").and_then(|v| v.as_str()).unwrap_or("Unknown").to_string();
        let confidence = result.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.0);

        let flagged = contradicts && confidence >= 0.85;
        if flagged {
            if let Err(e) = self.vault.lock().unwrap().flag_contradiction(
                item_id, older_id, &reason, confidence
            ) {
                ctx.audit("vault_detect_contradiction_error", json!({
                    "item_id": item_id,
                    "older_id": older_id,
                    "error": &e.to_string(),
                    "actor": ctx.actor,
                    "session_id": ctx.session_id,
                }));
                return Err(e.to_string());
            }
        }

        ctx.audit("vault_detect_contradiction", json!({
            "item_id": item_id,
            "older_id": older_id,
            "contradicts": contradicts,
            "confidence": confidence,
            "flagged": flagged,
            "reason": &reason,
            "actor": ctx.actor,
            "session_id": ctx.session_id,
        }));
        Ok(json!({
            "ok": true,
            "item_id": item_id,
            "older_version_id": older_id,
            "contradicts": contradicts,
            "confidence": confidence,
            "reason": reason,
            "flagged": flagged,
        }))
    }
}
