use serde_json;
use crate::tools::{Tool, ToolContext};
use std::sync::{Arc, Mutex};

pub struct VisionSearchTool {
    vision: Arc<Mutex<crate::vision::VisionService>>,
}

impl VisionSearchTool {
    pub fn new(vision: Arc<Mutex<crate::vision::VisionService>>) -> Self {
        Self { vision }
    }
}

#[async_trait::async_trait]
impl Tool for VisionSearchTool {
    fn name(&self) -> &str {
        "vision_search"
    }

    fn description(&self) -> &str {
        "Searches the VisionVault for images by description or tags. Returns matching image records with metadata."
    }

    fn is_core(&self) -> bool {
        false
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        _ctx: &ToolContext<'_>,
    ) -> Result<serde_json::Value, String> {
        let query = input
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or("Missing 'query' argument")?;
        let limit = input
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as usize;

        let results = self
            .vision
            .lock()
            .unwrap()
            .search(query, limit)
            .map_err(|e| e.to_string())?;

        Ok(serde_json::json!({ "images": results }))
    }
}
