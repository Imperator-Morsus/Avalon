use serde_json;
use crate::tools::{Tool, ToolContext};
use std::sync::{Arc, Mutex};

pub struct BoardPostTool {
    registry: Arc<Mutex<crate::agents::AgentRegistry>>,
}

impl BoardPostTool {
    pub fn new(registry: Arc<Mutex<crate::agents::AgentRegistry>>) -> Self {
        Self { registry }
    }
}

#[async_trait::async_trait]
impl Tool for BoardPostTool {
    fn name(&self) -> &str {
        "board_post"
    }

    fn description(&self) -> &str {
        "Posts a message to an agent dispatch board. Requires dispatch_id, author, and content. Optional channel parameter (default 'general')."
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
        let author = input
            .get("author")
            .and_then(|v| v.as_str())
            .ok_or("Missing 'author' argument")?;
        let content = input
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or("Missing 'content' argument")?;
        let channel = input
            .get("channel")
            .and_then(|v| v.as_str())
            .unwrap_or("general");

        let post_id = self
            .registry
            .lock()
            .unwrap()
            .post_to_board(dispatch_id, author, channel, content)
            .map_err(|e| e.to_string())?;

        Ok(serde_json::json!({
            "post_id": post_id,
            "dispatch_id": dispatch_id,
            "author": author,
            "channel": channel
        }))
    }
}
