use serde_json::json;
use crate::tools::{Tool, ToolContext};
use crate::vault::VaultService;
use std::sync::{Arc, Mutex};

// ═════════════════════════════════════════════════════════════════════════════
// vault_read Tool
// Retrieves a full document from the MindVault by ID.
// ═════════════════════════════════════════════════════════════════════════════

pub struct VaultReadTool {
    vault: Arc<Mutex<VaultService>>,
}

impl VaultReadTool {
    pub fn new(vault: Arc<Mutex<VaultService>>) -> Self {
        Self { vault }
    }
}

#[async_trait::async_trait]
impl Tool for VaultReadTool {
    fn name(&self) -> &str {
        "vault_read"
    }

    fn description(&self) -> &str {
        "Read a full document from the MindVault by its document ID. Returns the complete text content."
    }

    fn is_core(&self) -> bool {
        false
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        ctx: &ToolContext<'_>,
    ) -> Result<serde_json::Value, String> {
        let id = input
            .get("id")
            .and_then(|v| v.as_i64())
            .ok_or("Missing 'id' argument. Provide the document ID from vault_search.")?;

        let doc = match self.vault.lock().unwrap().get_filtered(id, &ctx.permission_level) {
            Ok(Some(d)) => d,
            Ok(None) => {
                let e = "Access denied or document not found.".to_string();
                ctx.audit("vault_read_error", json!({
                    "id": id,
                    "error": &e,
                    "actor": ctx.actor,
                    "session_id": ctx.session_id,
                }));
                return Err(e);
            }
            Err(e) => {
                ctx.audit("vault_read_error", json!({
                    "id": id,
                    "error": &e,
                    "actor": ctx.actor,
                    "session_id": ctx.session_id,
                }));
                return Err(e);
            }
        };
        ctx.audit("vault_read", json!({
            "id": doc.id,
            "source_path": doc.source_path,
            "actor": ctx.actor,
            "session_id": ctx.session_id,
        }));
        Ok(json!({
            "id": doc.id,
            "source_path": doc.source_path,
            "title": doc.title,
            "content_type": doc.content_type,
            "content": doc.content,
            "size_bytes": doc.size_bytes,
            "ingested_at": doc.ingested_at,
            "access_tier": doc.access_tier,
        }))
    }
}
