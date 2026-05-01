use serde_json::json;
use crate::tools::{Tool, ToolContext};
use crate::vault::VaultService;
use std::sync::{Arc, Mutex};

// ═════════════════════════════════════════════════════════════════════════════
// vault_link_items Tool
// Creates a relationship (graph edge) between two vault items.
// ═════════════════════════════════════════════════════════════════════════════

pub struct VaultLinkTool {
    vault: Arc<Mutex<VaultService>>,
}

impl VaultLinkTool {
    pub fn new(vault: Arc<Mutex<VaultService>>) -> Self {
        Self { vault }
    }
}

#[async_trait::async_trait]
impl Tool for VaultLinkTool {
    fn name(&self) -> &str {
        "vault_link_items"
    }

    fn description(&self) -> &str {
        "Create a relationship between two vault items. Input: {\"source_id\": 1, \"target_id\": 2, \"relation_type\": \"relates_to\", \"confidence\": 1.0, \"reason\": \"optional\"}"
    }

    fn is_core(&self) -> bool {
        false
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        ctx: &ToolContext<'_>,
    ) -> Result<serde_json::Value, String> {
        let source_id = input.get("source_id")
            .and_then(|v| v.as_i64())
            .ok_or("Missing 'source_id' argument.")?;
        let target_id = input.get("target_id")
            .and_then(|v| v.as_i64())
            .ok_or("Missing 'target_id' argument.")?;
        let relation_type = input.get("relation_type")
            .and_then(|v| v.as_str())
            .unwrap_or("relates_to");
        let confidence = input.get("confidence")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0);
        let reason = input.get("reason").and_then(|v| v.as_str());

        // Verify actor has permission to both items
        let vault = self.vault.lock().unwrap();
        if vault.get_filtered(source_id, &ctx.permission_level)?.is_none() {
            let e = "Access denied or source item not found.".to_string();
            drop(vault);
            ctx.audit("vault_link_error", json!({
                "source_id": source_id,
                "target_id": target_id,
                "relation_type": relation_type,
                "error": &e,
                "actor": ctx.actor,
                "session_id": ctx.session_id,
            }));
            return Err(e);
        }
        if vault.get_filtered(target_id, &ctx.permission_level)?.is_none() {
            let e = "Access denied or target item not found.".to_string();
            drop(vault);
            ctx.audit("vault_link_error", json!({
                "source_id": source_id,
                "target_id": target_id,
                "relation_type": relation_type,
                "error": &e,
                "actor": ctx.actor,
                "session_id": ctx.session_id,
            }));
            return Err(e);
        }
        drop(vault);

        let id = match self.vault.lock().unwrap().link_items(
            source_id, target_id, relation_type, confidence, reason
        ) {
            Ok(id) => {
                ctx.audit("vault_link", json!({
                    "source_id": source_id,
                    "target_id": target_id,
                    "relation_type": relation_type,
                    "relationship_id": id,
                    "actor": ctx.actor,
                    "session_id": ctx.session_id,
                }));
                id
            }
            Err(e) => {
                ctx.audit("vault_link_error", json!({
                    "source_id": source_id,
                    "target_id": target_id,
                    "relation_type": relation_type,
                    "error": &e,
                    "actor": ctx.actor,
                    "session_id": ctx.session_id,
                }));
                return Err(e);
            }
        };

        Ok(json!({
            "ok": true,
            "relationship_id": id,
            "source_id": source_id,
            "target_id": target_id,
            "relation_type": relation_type,
        }))
    }
}
