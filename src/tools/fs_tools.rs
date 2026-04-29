use serde_json;
use crate::tools::{Tool, ToolContext};

pub struct ReadFileTool;
pub struct WriteFileTool;
pub struct ListDirTool;
pub struct DeleteFileTool;

#[async_trait::async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Reads the contents of a file at the given path."
    }

    fn is_core(&self) -> bool { true }

    async fn execute(&self, input: serde_json::Value, ctx: &ToolContext<'_>) -> Result<serde_json::Value, String> {
        let path = input.get("path").and_then(|v| v.as_str()).ok_or("Missing path")?;
        let result = ctx.fs.read_file(path);
        serde_json::to_value(result).map_err(|e| e.to_string())
    }
}

#[async_trait::async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Writes or overwrites a file at the given path with the provided content."
    }

    fn is_core(&self) -> bool { true }

    async fn execute(&self, input: serde_json::Value, ctx: &ToolContext<'_>) -> Result<serde_json::Value, String> {
        let path = input.get("path").and_then(|v| v.as_str()).ok_or("Missing path")?;
        let content = input.get("content").and_then(|v| v.as_str()).ok_or("Missing content")?;
        if ctx.security.require_write_permission {
            return Ok(serde_json::json!({
                "success": false,
                "path": path,
                "content": null,
                "error": "Write operations require explicit permission. This is enforced by the Security settings (require_write_permission). You can approve this in Settings > Security or ask the user to disable the write permission gate.",
                "entries": null
            }));
        }
        let result = ctx.fs.write_file(path, content);

        // Auto-ingest into MindVault if write succeeded
        if result.success {
            let normalized = crate::fs::normalize_path(path);
            let _ = ctx.vault.lock().unwrap().ingest_file(
                std::path::Path::new(&normalized),
                None,
                None,
            );
        }

        serde_json::to_value(result).map_err(|e| e.to_string())
    }
}

#[async_trait::async_trait]
impl Tool for ListDirTool {
    fn name(&self) -> &str {
        "list_dir"
    }

    fn description(&self) -> &str {
        "Lists all files and directories in the given path."
    }

    fn is_core(&self) -> bool { true }

    async fn execute(&self, input: serde_json::Value, ctx: &ToolContext<'_>) -> Result<serde_json::Value, String> {
        let path = input.get("path").and_then(|v| v.as_str()).ok_or("Missing path")?;
        let result = ctx.fs.list_dir(path);
        serde_json::to_value(result).map_err(|e| e.to_string())
    }
}

#[async_trait::async_trait]
impl Tool for DeleteFileTool {
    fn name(&self) -> &str {
        "delete_file"
    }

    fn description(&self) -> &str {
        "Deletes a file or directory at the given path."
    }

    fn is_core(&self) -> bool { true }

    async fn execute(&self, input: serde_json::Value, ctx: &ToolContext<'_>) -> Result<serde_json::Value, String> {
        let path = input.get("path").and_then(|v| v.as_str()).ok_or("Missing path")?;
        if ctx.security.require_delete_permission {
            return Ok(serde_json::json!({
                "success": false,
                "path": path,
                "content": null,
                "error": "Delete operations require explicit permission. This is enforced by the Security settings (require_delete_permission). You can approve this in Settings > Security or ask the user to disable the delete permission gate.",
                "entries": null
            }));
        }
        let result = ctx.fs.delete_file(path);
        serde_json::to_value(result).map_err(|e| e.to_string())
    }
}
