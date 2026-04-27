use serde_json;
use crate::tools::{Tool, ToolContext};

pub struct GetFsConfigTool;

#[async_trait::async_trait]
impl Tool for GetFsConfigTool {
    fn name(&self) -> &str {
        "get_fs_config"
    }

    fn description(&self) -> &str {
        "Reads the current file system limiter configuration including allowed/denied paths and max file size."
    }

    fn is_core(&self) -> bool { true }

    async fn execute(&self, _input: serde_json::Value, ctx: &ToolContext<'_>) -> Result<serde_json::Value, String> {
        let cfg = ctx.fs.config();
        serde_json::to_value(cfg).map_err(|e| e.to_string())
    }
}
