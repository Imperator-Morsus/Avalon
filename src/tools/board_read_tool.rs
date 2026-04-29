use serde_json;
use crate::tools::{Tool, ToolContext};
use std::sync::{Arc, Mutex};

pub struct BoardReadTool {
    registry: Arc<Mutex<crate::agents::AgentRegistry>>,
}

impl BoardReadTool {
    pub fn new(registry: Arc<Mutex<crate::agents::AgentRegistry>>) -> Self {
        Self { registry }
    }
}

#[async_trait::async_trait]
impl Tool for BoardReadTool {
    fn name(&self) -> &str {
        "board_read"
    }

    fn description(&self) -> &str {
        "Reads messages from an agent dispatch board. Requires dispatch_id. Optional channel and since parameters."
    }

    fn is_core(&self) -> bool {
        false
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        _ctx: &ToolContext<'_>,
    ) -> Result<serde_json::Value, String> {
        let dispatch_id = input
            .get("dispatch_id")
            .and_then(|v| v.as_i64())
            .ok_or("Missing 'dispatch_id' argument")?;
        let channel = input
            .get("channel")
            .and_then(|v| v.as_str());
        let since = input
            .get("since")
            .and_then(|v| v.as_str());

        let posts = self
            .registry
            .lock()
            .unwrap()
            .read_board(dispatch_id, channel, since)
            .map_err(|e| e.to_string())?;

        Ok(serde_json::json!({
            "dispatch_id": dispatch_id,
            "posts": posts
        }))
    }
}
