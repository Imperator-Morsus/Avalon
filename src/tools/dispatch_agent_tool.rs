use serde_json;
use crate::tools::{Tool, ToolContext};
use std::sync::{Arc, Mutex};

pub struct DispatchAgentTool {
    registry: Arc<Mutex<crate::agents::AgentRegistry>>,
}

impl DispatchAgentTool {
    pub fn new(registry: Arc<Mutex<crate::agents::AgentRegistry>>) -> Self {
        Self { registry }
    }
}

#[async_trait::async_trait]
impl Tool for DispatchAgentTool {
    fn name(&self) -> &str {
        "dispatch_agent"
    }

    fn description(&self) -> &str {
        "Dispatches an agent to perform a task. Creates a dispatch record and returns the dispatch ID. The agent will only be able to use tools in its allowed_tools whitelist."
    }

    fn is_core(&self) -> bool {
        false
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        _ctx: &ToolContext<'_>,
    ) -> Result<serde_json::Value, String> {
        let agent_name = input
            .get("agent_name")
            .and_then(|v| v.as_str())
            .ok_or("Missing 'agent_name' argument")?;
        let task = input
            .get("task")
            .and_then(|v| v.as_str())
            .ok_or("Missing 'task' argument")?;

        let dispatch_id = self
            .registry
            .lock()
            .unwrap()
            .create_dispatch(agent_name, task)
            .map_err(|e| e.to_string())?;

        Ok(serde_json::json!({
            "dispatch_id": dispatch_id,
            "agent_name": agent_name,
            "task": task,
            "status": "pending"
        }))
    }
}
