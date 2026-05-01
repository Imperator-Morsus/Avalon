use serde_json::json;
use crate::tools::{Tool, ToolContext};
use crate::vault::VaultService;
use std::sync::{Arc, Mutex};

// ═════════════════════════════════════════════════════════════════════════════
// vault_search Tool
// Searches the MindVault using SQLite FTS5.
// ═════════════════════════════════════════════════════════════════════════════

pub struct VaultSearchTool {
    vault: Arc<Mutex<VaultService>>,
}

impl VaultSearchTool {
    pub fn new(vault: Arc<Mutex<VaultService>>) -> Self {
        Self { vault }
    }
}

#[async_trait::async_trait]
impl Tool for VaultSearchTool {
    fn name(&self) -> &str {
        "vault_search"
    }

    fn description(&self) -> &str {
        "Search the MindVault for documents by full-text query. Returns matching documents with excerpts."
    }

    fn is_core(&self) -> bool {
        false
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        ctx: &ToolContext<'_>,
    ) -> Result<serde_json::Value, String> {
        let query = input
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or("Missing 'query' argument.")?;

        let limit = input
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as usize;

        let results = match self.vault.lock().unwrap()
            .search_filtered(query, limit, &ctx.permission_level) {
            Ok(r) => r,
            Err(e) => {
                ctx.audit("vault_search_error", json!({
                    "query": query,
                    "error": &e,
                    "actor": ctx.actor,
                    "session_id": ctx.session_id,
                }));
                return Err(e);
            }
        };

        let count = results.len();
        ctx.audit("vault_search", json!({
            "query": query,
            "results_returned": count,
            "actor": ctx.actor,
            "session_id": ctx.session_id,
        }));

        let docs: Vec<serde_json::Value> = results.iter().map(|doc| {
            json!({
                "id": doc.id,
                "source_path": doc.source_path,
                "title": doc.title,
                "content_type": doc.content_type,
                "size_bytes": doc.size_bytes,
                "ingested_at": doc.ingested_at,
                "access_tier": doc.access_tier,
            })
        }).collect();

        Ok(json!({
            "count": docs.len(),
            "results": docs,
        }))
    }
}
