use serde_json::json;
use crate::tools::{Tool, ToolContext};
use crate::vault::VaultService;
use std::sync::{Arc, Mutex};

// ═════════════════════════════════════════════════════════════════════════════
// vault_ingest Tool
// Ingests a file from disk into The Vault.
// ═════════════════════════════════════════════════════════════════════════════

pub struct VaultIngestTool {
    vault: Arc<Mutex<VaultService>>,
}

impl VaultIngestTool {
    pub fn new(vault: Arc<Mutex<VaultService>>) -> Self {
        Self { vault }
    }
}

#[async_trait::async_trait]
impl Tool for VaultIngestTool {
    fn name(&self) -> &str {
        "vault_ingest"
    }

    fn description(&self) -> &str {
        "Ingest a file from disk into The Vault. Input: {\"path\": \"path/to/file.txt\", \"title\": \"optional title\", \"content_type\": \"optional hint\"}"
    }

    fn is_core(&self) -> bool {
        false
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        ctx: &ToolContext<'_>,
    ) -> Result<serde_json::Value, String> {
        let path_str = input.get("path")
            .and_then(|v| v.as_str())
            .ok_or("Missing 'path' argument.")?;
        let title = input.get("title").and_then(|v| v.as_str());
        let content_type_hint = input.get("content_type").and_then(|v| v.as_str());

        // Validate path is in allowed_paths
        if !ctx.fs.config().is_allowed(path_str) {
            let e = format!(
                "Path '{}' is not in allowed_paths. Add it to filesystem configuration to enable access.",
                path_str
            );
            ctx.audit("vault_ingest_error", json!({
                "path": path_str,
                "error": &e,
                "actor": ctx.actor,
                "session_id": ctx.session_id,
            }));
            return Err(e);
        }

        let path = std::path::Path::new(path_str);
        if !path.exists() {
            let e = format!("File not found: {}", path_str);
            ctx.audit("vault_ingest_error", json!({
                "path": path_str,
                "error": &e,
                "actor": ctx.actor,
                "session_id": ctx.session_id,
            }));
            return Err(e);
        }

        let access_tier = ctx.permission_level.as_str();
        let owner_id = Some(ctx.actor);
        let id = match self.vault.lock().unwrap()
            .ingest_file(path, title, content_type_hint, access_tier, owner_id) {
            Ok(id) => {
                ctx.audit("vault_ingest", json!({
                    "path": path_str,
                    "id": id,
                    "content_type": access_tier,
                    "actor": ctx.actor,
                    "session_id": ctx.session_id,
                }));
                id
            }
            Err(e) => {
                ctx.audit("vault_ingest_error", json!({
                    "path": path_str,
                    "error": &e,
                    "actor": ctx.actor,
                    "session_id": ctx.session_id,
                }));
                return Err(e);
            }
        };

        Ok(json!({
            "ok": true,
            "id": id,
            "path": path_str,
        }))
    }
}
