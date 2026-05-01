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
        "Scans the allowed file system paths and builds a graph of files and their relationships (imports, references, directory structure). Returns the mind map as JSON with nodes and edges. To scan a specific directory, pass {\"path\": \"C:/some/dir\"} as input. If no path is given, scans the default allowed paths."
    }

    fn is_core(&self) -> bool { false }

    async fn execute(&self, input: serde_json::Value, ctx: &ToolContext<'_>) -> Result<serde_json::Value, String> {
        let mut mm = MindMapService::new();
        // Accept both "path" and "root" since some models confuse the mindmap output field with the input parameter
        let path_val = input.get("path").and_then(|v| v.as_str())
            .or_else(|| input.get("root").and_then(|v| v.as_str()));
        if let Some(path) = path_val {
            if !ctx.fs.config().is_allowed(path) {
                return Err(format!("Path '{}' is not allowed by the file system limiter. Add it to allowed_paths in Settings > File System Limiter.", path));
            }
            mm.build(&[path.to_string()], 3);
        } else {
            let allowed: Vec<String> = ctx.fs.config().allowed_paths.clone();
            mm.build(&allowed, 3);
        }
        serde_json::to_value(mm.graph()).map_err(|e| e.to_string())
    }
}
