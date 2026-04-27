use serde_json;
use crate::mindmap::MindMapService;
use crate::tools::{Tool, ToolContext};

pub struct MindMapTool;

#[async_trait::async_trait]
impl Tool for MindMapTool {
    fn name(&self) -> &str {
        "mindmap"
    }

    fn description(&self) -> &str {
        "Scans the allowed file system paths and builds a graph of files and their relationships (imports, references, directory structure). Returns the mind map as JSON with nodes and edges."
    }

    fn is_core(&self) -> bool { false }

    async fn execute(&self, _input: serde_json::Value, ctx: &ToolContext<'_>) -> Result<serde_json::Value, String> {
        let mut mm = MindMapService::new();
        let allowed: Vec<String> = ctx.fs.config().allowed_paths.clone();
        mm.build(&allowed, 3);
        serde_json::to_value(mm.graph()).map_err(|e| e.to_string())
    }
}
