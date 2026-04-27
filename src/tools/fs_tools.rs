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

    async fn execute(&self, input: serde_json::Value, ctx: &ToolContext<'_>) -> Result<serde_json::Value, String> {
        let path = input.get("path").and_then(|v| v.as_str()).ok_or("Missing path")?;
        let content = input.get("content").and_then(|v| v.as_str()).ok_or("Missing content")?;
        let result = ctx.fs.write_file(path, content);
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

    async fn execute(&self, input: serde_json::Value, ctx: &ToolContext<'_>) -> Result<serde_json::Value, String> {
        let path = input.get("path").and_then(|v| v.as_str()).ok_or("Missing path")?;
        let result = ctx.fs.delete_file(path);
        serde_json::to_value(result).map_err(|e| e.to_string())
    }
}
