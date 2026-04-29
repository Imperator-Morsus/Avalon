use serde_json;
use crate::tools::{Tool, ToolContext};
use std::sync::{Arc, Mutex};

pub struct VisionReadTool {
    vision: Arc<Mutex<crate::vision::VisionService>>,
}

impl VisionReadTool {
    pub fn new(vision: Arc<Mutex<crate::vision::VisionService>>) -> Self {
        Self { vision }
    }
}

#[async_trait::async_trait]
impl Tool for VisionReadTool {
    fn name(&self) -> &str {
        "vision_read"
    }

    fn description(&self) -> &str {
        "Retrieves a single image record from the VisionVault by ID. Returns metadata including description, tags, and path."
    }

    fn is_core(&self) -> bool {
        false
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        _ctx: &ToolContext<'_>,
    ) -> Result<serde_json::Value, String> {
        let id = input
            .get("id")
            .and_then(|v| v.as_i64())
            .ok_or("Missing 'id' argument")?;

        match self
            .vision
            .lock()
            .unwrap()
            .get(id)
            .map_err(|e| e.to_string())?
        {
            Some(img) => {
                serde_json::to_value(img).map_err(|e| e.to_string())
            }
            None => Err(format!("Image with id {} not found", id)),
        }
    }
}
