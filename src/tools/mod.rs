use std::collections::HashMap;
use serde_json;

use crate::fs::FileSystemService;

pub mod fs_tools;
pub mod config_tool;
pub mod mindmap_tool;

pub struct ToolContext<'a> {
    pub fs: &'a FileSystemService,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
}

#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
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
            })
            .collect()
    }

    pub fn names(&self) -> Vec<&str> {
        self.tools.values().map(|t| t.name()).collect()
    }
}
