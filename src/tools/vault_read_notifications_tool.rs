use serde_json::json;
use crate::tools::{Tool, ToolContext};
use crate::vault::VaultService;
use std::sync::{Arc, Mutex};

// ═════════════════════════════════════════════════════════════════════════════
// vault_read_notifications Tool
// Returns unread notifications from The Vault.
// ═════════════════════════════════════════════════════════════════════════════

pub struct VaultReadNotificationsTool {
    vault: Arc<Mutex<VaultService>>,
}

impl VaultReadNotificationsTool {
    pub fn new(vault: Arc<Mutex<VaultService>>) -> Self {
        Self { vault }
    }
}

#[async_trait::async_trait]
impl Tool for VaultReadNotificationsTool {
    fn name(&self) -> &str {
        "vault_read_notifications"
    }

    fn description(&self) -> &str {
        "Read unread notifications from The Vault. Input: {\"limit\": 20}. Returns notifications with type, message, and item_id."
    }

    fn is_core(&self) -> bool {
        false
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        ctx: &ToolContext<'_>,
    ) -> Result<serde_json::Value, String> {
        let limit = input.get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(20) as usize;

        let notifs = self.vault.lock().unwrap().get_unread_notifications(limit)
            .map_err(|e| e.to_string())?;

        ctx.audit("vault_read_notifications", json!({
            "count": notifs.len(),
            "actor": ctx.actor,
            "session_id": ctx.session_id,
        }));

        Ok(json!({
            "count": notifs.len(),
            "notifications": notifs,
        }))
    }
}
