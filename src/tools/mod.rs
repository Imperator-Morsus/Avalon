use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use serde_json;

use crate::fs::FileSystemService;
use crate::audit::AuditLog;

pub mod fs_tools;
pub mod config_tool;
pub mod mindmap_tool;
pub mod fetch_tool;
pub mod remote_mindmap_tool;
pub mod web_scrape_tool;
pub mod video_tool;
pub mod vault_search_tool;
pub mod vault_read_tool;
pub mod vault_ingest_tool;
pub mod vault_link_tool;
pub mod vault_read_notifications_tool;
pub mod vault_extract_concepts_tool;
pub mod vault_detect_contradiction_tool;
pub mod dispatch_agent_tool;
pub mod board_post_tool;
pub mod board_read_tool;
pub mod transcribe_tool;

pub struct ToolContext<'a> {
    pub fs: &'a FileSystemService,
    pub web_fetch: &'a crate::WebFetchConfig,
    pub security: &'a crate::SecurityConfig,
    pub mindmap: &'a std::sync::Mutex<crate::mindmap::MindMapService>,
    pub vault: &'a std::sync::Mutex<crate::vault::VaultService>,
    pub actor: &'a str,                            // "user", "assistant", or agent name
    pub session_id: &'a str,                       // opaque session identifier
    pub permission_level: crate::db::AccessTier,  // actor's vault clearance level
    pub audit_log: Option<&'a Arc<Mutex<AuditLog>>>, // optional audit logger
}

impl<'a> ToolContext<'a> {
    pub fn new(
        fs: &'a FileSystemService,
        web_fetch: &'a crate::WebFetchConfig,
        security: &'a crate::SecurityConfig,
        mindmap: &'a std::sync::Mutex<crate::mindmap::MindMapService>,
        vault: &'a std::sync::Mutex<crate::vault::VaultService>,
        actor: &'a str,
        session_id: &'a str,
        permission_level: crate::db::AccessTier,
        audit_log: Option<&'a Arc<Mutex<AuditLog>>>,
    ) -> Self {
        Self { fs, web_fetch, security, mindmap, vault, actor, session_id, permission_level, audit_log }
    }

    /// Log an audit entry using the embedded actor and session_id.
    /// No-op if audit_log is None.
    pub fn audit(&self, entry_type: &str, data: serde_json::Value) {
        if let Some(log) = self.audit_log {
            if let Ok(mut guard) = log.lock() {
                guard.push_with_actor(entry_type, self.actor, data);
            }
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub is_core: bool,
}

#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn is_core(&self) -> bool { true }
    async fn execute(&self, input: serde_json::Value, ctx: &ToolContext<'_>) -> Result<serde_json::Value, String>;
}

pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register(&mut self, tool: Box<dyn Tool>) {
        let name = tool.name().to_string();
        self.tools.insert(name, tool);
    }

    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    pub fn list(&self) -> Vec<ToolInfo> {
        self.tools.values()
            .map(|t| ToolInfo {
                name: t.name().to_string(),
                description: t.description().to_string(),
                is_core: t.is_core(),
            })
            .collect()
    }

    pub fn names(&self) -> Vec<&str> {
        self.tools.values().map(|t| t.name()).collect()
    }
}
