mod fs;
mod tools;
mod mindmap;

use actix_cors::Cors;
use actix_web::{web, App, HttpResponse, HttpServer};
use futures_util::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;

use std::collections::{HashMap, HashSet};
use std::env;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use fs::{FileSystemService, FileSystemConfig};
use mindmap::MindMapService;

// ═════════════════════════════════════════════════════════════════════════════
// Data Contracts
// ═════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InferenceRequest {
    prompt: String,
    #[serde(default)]
    user_context: String,
    #[serde(default)]
    mindmap_payload: serde_json::Value,
    #[serde(default)]
    image_archives: Vec<serde_json::Value>,
    #[serde(default)]
    other_instances: serde_json::Value,
    #[serde(default)]
    model_params: serde_json::Value,
    #[serde(default = "default_ai_name")]
    ai_name: String,
}

fn default_ai_name() -> String { "Avalon".to_string() }

#[derive(Debug, Clone, Serialize)]
struct InferenceResponse {
    completion: String,
    model_used: String,
}

// ═════════════════════════════════════════════════════════════════════════════
// Security Manager Service (backend enforcement layer)
// ═════════════════════════════════════════════════════════════════════════════

type ModelId = String;

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct FileAccessRequest {
    action: String,
    path: String,
    calling_module: String,
    owner: ModelId,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum AccessPermissions {
    ReadOnly,
    WriteOnly,
    ReadWrite,
    None,
}

pub struct SecurityManager {
    permissions: HashMap<String, HashMap<String, AccessPermissions>>,
}

impl SecurityManager {
    pub fn new() -> Self {
        let mut manager = SecurityManager {
            permissions: HashMap::new(),
        };
        let mut default_permissions = HashMap::new();
        default_permissions.insert("src/main.rs".to_string(), AccessPermissions::ReadOnly);
        manager.permissions.insert("core_system".to_string(), default_permissions);
        manager
    }

    pub fn check_access(&self, request: &FileAccessRequest) -> Result<bool, String> {
        let module_permissions = self.permissions.get(&request.calling_module)
            .ok_or_else(|| format!("Unknown module: {}", request.calling_module))?;
        let path_permissions = module_permissions.get(&request.path)
            .cloned()
            .unwrap_or(AccessPermissions::None);

        match (request.action.as_str(), path_permissions) {
            ("read", AccessPermissions::ReadOnly) |
            ("read", AccessPermissions::ReadWrite) |
            ("write", AccessPermissions::ReadWrite) |
            ("delete", AccessPermissions::ReadWrite) => Ok(true),
            ("read", _) => Ok(matches!(path_permissions, AccessPermissions::ReadOnly | AccessPermissions::ReadWrite)),
            ("write", _) => Ok(matches!(path_permissions, AccessPermissions::ReadWrite | AccessPermissions::WriteOnly)),
            ("delete", _) => Ok(matches!(path_permissions, AccessPermissions::ReadWrite)),
            _ => Ok(false),
        }
    }

    pub fn register_permission(&mut self, module: &str, path: &str, permissions: AccessPermissions) {
        let entry = self.permissions.entry(module.to_string()).or_default();
        entry.insert(path.to_string(), permissions);
    }

    pub fn remove_permission(&mut self, module: &str, path: &str) {
        if let Some(module_perms) = self.permissions.get_mut(module) {
            module_perms.remove(path);
            if module_perms.is_empty() {
                self.permissions.remove(module);
            }
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Session Permission Manager (UI-driven, session-scoped approvals)
// ═════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize)]
pub struct SessionPermission {
    tool: String,
    permissions: AccessPermissions,
    granted_at: u64,
}

pub struct SessionPermissionManager {
    approved: HashMap<String, AccessPermissions>,
    pending: Option<String>,
}

impl SessionPermissionManager {
    pub fn new() -> Self {
        SessionPermissionManager {
            approved: HashMap::new(),
            pending: None,
        }
    }

    pub fn approve(&mut self, tool: &str, permissions: AccessPermissions) {
        self.approved.insert(tool.to_string(), permissions);
        self.pending = None;
    }

    pub fn deny(&mut self, tool: &str) {
        self.approved.remove(tool);
        self.pending = None;
    }

    pub fn is_approved(&self, tool: &str, action: &str) -> bool {
        match self.approved.get(tool) {
            Some(AccessPermissions::ReadWrite) => true,
            Some(AccessPermissions::WriteOnly) if action == "write" => true,
            Some(AccessPermissions::ReadOnly) if action == "read" => true,
            _ => false,
        }
    }

    pub fn set_pending(&mut self, tool: String) {
        self.pending = Some(tool);
    }

    pub fn get_pending(&self) -> Option<&String> {
        self.pending.as_ref()
    }

    pub fn revoke(&mut self, tool: &str) {
        self.approved.remove(tool);
    }

    pub fn list(&self) -> Vec<SessionPermission> {
        self.approved.iter().map(|(tool, perm)| SessionPermission {
            tool: tool.clone(),
            permissions: *perm,
            granted_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
        }).collect()
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Debug Log
// ═════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize)]
pub struct DebugEntry {
    #[serde(rename = "type")]
    entry_type: String,
    data: serde_json::Value,
}

pub struct DebugLog {
    entries: Vec<DebugEntry>,
    max_size: usize,
}

impl DebugLog {
    pub fn new() -> Self {
        DebugLog {
            entries: Vec::new(),
            max_size: 10000,
        }
    }

    pub fn push(&mut self, entry_type: &str, data: serde_json::Value) {
        self.entries.push(DebugEntry {
            entry_type: entry_type.to_string(),
            data,
        });
        if self.entries.len() > self.max_size {
            let excess = self.entries.len() - self.max_size;
            self.entries.drain(0..excess);
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn get_all(&self) -> &Vec<DebugEntry> {
        &self.entries
    }

    pub fn save_to_file(&self) -> Result<String, String> {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let filename = format!("avalon-debug-{}.md", timestamp);
        let project_dir = std::env::current_exe()
            .ok()
            .and_then(|p| {
                let mut path = p;
                path.pop(); // target/release
                path.pop(); // target
                path.pop(); // project root
                Some(path)
            })
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let logs_dir = project_dir.join("logs");
        if let Err(e) = std::fs::create_dir_all(&logs_dir) {
            return Err(format!("Failed to create logs directory: {}", e));
        }
        let path = logs_dir.join(&filename);

        let mut md = String::new();
        md.push_str(&format!("# Avalon Debug Log\n\n"));
        md.push_str(&format!("**Generated:** {}\n\n", timestamp));
        md.push_str("---\n\n");

        for entry in &self.entries {
            md.push_str(&format!("## {}\n\n", entry.entry_type));
            if let Ok(pretty) = serde_json::to_string_pretty(&entry.data) {
                md.push_str("```json\n");
                md.push_str(&pretty);
                md.push_str("\n```\n\n");
            } else {
                md.push_str(&format!("```\n{:?}\n```\n\n", entry.data));
            }
        }

        std::fs::write(&path, md)
            .map_err(|e| format!("Failed to write debug log: {}", e))?;
        Ok(path.to_string_lossy().to_string())
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// App Config
// ═════════════════════════════════════════════════════════════════════════════

#[allow(dead_code)]
pub struct AppConfig {
    current_model: String,
    api_base: String,
    pub active_tools: Vec<String>,
    pub ai_name: String,
    pub web_fetch: WebFetchConfig,
}

impl AppConfig {
    pub fn new() -> Self {
        let persisted = ConfigStore::load();
        let current_model = env::var("AVALON_MODEL_NAME")
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| persisted.get("current_model").and_then(|v| v.as_str()).map(|s| s.to_string()))
            .unwrap_or_default();
        let active_tools = persisted.get("active_tools")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default();
        let ai_name = persisted.get("ai_name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "Avalon".to_string());
        let web_fetch = WebFetchConfig::load();
        AppConfig {
            current_model,
            api_base: env::var("AVALON_MODEL_API_BASE").unwrap_or_else(|_| "http://localhost:11434/v1".to_string()),
            active_tools,
            ai_name,
            web_fetch,
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Web Fetch Config
// ═════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WebFetchConfig {
    pub max_depth: u32,
    pub confirm_domains: bool,
    pub allowed_domains: Vec<String>,
    pub blocked_domains: Vec<String>,
    pub timeout_secs: u64,
    pub max_size_mb: u32,
    pub respect_robots_txt: bool,
    pub rate_limit_ms: u64,
}

impl Default for WebFetchConfig {
    fn default() -> Self {
        WebFetchConfig {
            max_depth: 1,
            confirm_domains: true,
            allowed_domains: vec![
                "github.com".to_string(),
                "raw.githubusercontent.com".to_string(),
                "gist.github.com".to_string(),
                "api.github.com".to_string(),
            ],
            blocked_domains: vec![],
            timeout_secs: 10,
            max_size_mb: 5,
            respect_robots_txt: true,
            rate_limit_ms: 1000,
        }
    }
}

impl WebFetchConfig {
    pub fn load() -> Self {
        let persisted = ConfigStore::load();
        persisted.get("web_fetch")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        let mut persisted = ConfigStore::load();
        persisted.insert("web_fetch".to_string(), json!(self));
        ConfigStore::save(&persisted);
        Ok(())
    }
}

pub struct ConfigStore;

impl ConfigStore {
    fn config_path() -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|p| {
                let mut path = p;
                path.pop();
                path.pop();
                path.pop();
                Some(path)
            })
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
            .join(".avalon_state.json")
    }

    pub fn load() -> HashMap<String, serde_json::Value> {
        let path = Self::config_path();
        if !path.exists() {
            return HashMap::new();
        }
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return HashMap::new(),
        };
        serde_json::from_str(&content).unwrap_or_default()
    }

    pub fn save(config: &HashMap<String, serde_json::Value>) {
        let path = Self::config_path();
        if let Ok(content) = serde_json::to_string_pretty(config) {
            let _ = std::fs::write(&path, content);
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Model Inference Service
// ═════════════════════════════════════════════════════════════════════════════

#[async_trait::async_trait]
pub trait ModelInferenceService: Send + Sync + 'static {
    async fn infer(&self, request: &InferenceRequest) -> Result<String, String>;
    fn model_name(&self) -> String;
    fn api_base_url(&self) -> String;
    fn as_any(&self) -> &dyn std::any::Any;
}

struct HttpModelService {
    client: reqwest::Client,
    api_base: String,
    raw_base: String,
    api_key: String,
    model_name: String,
    tools_description: String,
}

impl HttpModelService {
    fn new() -> Result<Self, String> {
        let api_key = env::var("AVALON_MODEL_API_KEY").unwrap_or_default();
        let api_base = env::var("AVALON_MODEL_API_BASE")
            .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());
        let raw_base = if api_base.ends_with("/v1") {
            api_base[..api_base.len() - 3].to_string()
        } else {
            api_base.clone()
        };
        let model_name = env::var("AVALON_MODEL_NAME")
            .unwrap_or_else(|_| "gpt-4o-mini".to_string());

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

        Ok(HttpModelService {
            client,
            api_base,
            raw_base,
            api_key,
            model_name,
            tools_description: String::new(),
        })
    }

    fn build_messages(&self, req: &InferenceRequest) -> Vec<serde_json::Value> {
        let system_msg = json!({
            "role": "system",
            "content": format!("You are {}, a context-aware AI coding assistant. Respond in Markdown.\n\nYou have access to the local file system via structured tool calls. When you need to read, write, or edit a file, output a tool call block like this:\n\n<tool>\n<name>read_file</name>\n<input>{{\"path\": \"src/main.rs\"}}</input>\n</tool>\n\n{}\n\nWhen you need to reason through a problem, plan code, or work through logic step-by-step, wrap your internal thinking in <thinking>...</thinking> tags. Provide your final answer after the closing </thinking> tag. Only the content outside the tags will be shown to the user.", req.ai_name, self.tools_description)
        });

        let mut content_parts: Vec<serde_json::Value> = Vec::new();
        content_parts.push(json!({
            "type": "text",
            "text": format!("## User Query\n{}", req.prompt)
        }));

        if !req.user_context.is_empty() {
            content_parts.push(json!({
                "type": "text",
                "text": format!("## User Context\n{}", req.user_context)
            }));
        }

        if !req.mindmap_payload.is_null() && req.mindmap_payload != json!({}) {
            content_parts.push(json!({
                "type": "text",
                "text": format!("## Mindmap Data\n{}", serde_json::to_string_pretty(&req.mindmap_payload).unwrap_or_default())
            }));
        }

        if !req.other_instances.is_null() && req.other_instances != json!({}) {
            content_parts.push(json!({
                "type": "text",
                "text": format!("## External Instances\n{}", serde_json::to_string_pretty(&req.other_instances).unwrap_or_default())
            }));
        }

        for (idx, img) in req.image_archives.iter().enumerate() {
            if let Some(b64) = img.get("base64").and_then(|v| v.as_str()) {
                let mime = img.get("mime_type").and_then(|v| v.as_str()).unwrap_or("image/png");
                content_parts.push(json!({
                    "type": "image_url",
                    "image_url": {
                        "url": format!("data:{};base64,{}", mime, b64)
                    }
                }));
            } else if let Some(url) = img.get("url").and_then(|v| v.as_str()) {
                content_parts.push(json!({
                    "type": "image_url",
                    "image_url": { "url": url }
                }));
            } else {
                content_parts.push(json!({
                    "type": "text",
                    "text": format!("## Image Archive #{}\n{}", idx + 1, serde_json::to_string_pretty(img).unwrap_or_default())
                }));
            }
        }

        let user_msg = json!({
            "role": "user",
            "content": content_parts
        });

        vec![system_msg, user_msg]
    }

    async fn ollama_list_models(&self) -> Result<Vec<String>, String> {
        let url = format!("{}/api/tags", self.raw_base);
        let resp = self.client.get(&url)
            .send().await
            .map_err(|e| format!("Failed to list models: {}", e))?;
        let status = resp.status();
        let text = resp.text().await
            .map_err(|e| format!("Failed to read response: {}", e))?;
        if !status.is_success() {
            return Err(format!("Ollama returned {}: {}", status, text));
        }
        let json: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| format!("Invalid JSON: {} | {}", e, text))?;
        let models = json["models"].as_array()
            .map(|arr| arr.iter()
                .filter_map(|m| m["name"].as_str().map(|s| s.to_string()))
                .collect())
            .unwrap_or_default();
        Ok(models)
    }

    async fn ollama_preload(&self, model: &str) -> Result<(), String> {
        let url = format!("{}/api/generate", self.raw_base);
        let body = json!({
            "model": model,
            "prompt": "",
            "keep_alive": -1
        });
        let resp = self.client.post(&url).json(&body).send().await
            .map_err(|e| format!("Preload request failed: {}", e))?;
        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Preload failed: {}", text));
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl ModelInferenceService for HttpModelService {
    async fn infer(&self, request: &InferenceRequest) -> Result<String, String> {
        let url = format!("{}/chat/completions", self.api_base);
        let messages = self.build_messages(request);

        let mut body = json!({
            "model": self.model_name,
            "messages": messages,
            "stream": false
        });

        if let Some(obj) = request.model_params.as_object() {
            if let Some(map) = body.as_object_mut() {
                for (k, v) in obj {
                    map.insert(k.clone(), v.clone());
                }
            }
        }

        let mut request_builder = self.client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body);

        if !self.api_key.is_empty() {
            request_builder = request_builder.header("Authorization", format!("Bearer {}", self.api_key));
        }

        let response = request_builder.send().await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        let status = response.status();
        let text = response.text().await
            .map_err(|e| format!("Failed to read response body: {}", e))?;

        if !status.is_success() {
            return Err(format!("API returned {}: {}", status, text));
        }

        let json_resp: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| format!("Failed to parse JSON response: {} | body: {}", e, text))?;

        let completion = json_resp["choices"]
            .get(0)
            .and_then(|c| c["message"]["content"].as_str())
            .ok_or_else(|| format!("Unexpected API response format: {}", text))?
            .to_string();

        Ok(completion)
    }

    fn model_name(&self) -> String {
        self.model_name.clone()
    }

    fn api_base_url(&self) -> String {
        self.api_base.clone()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

struct DummyModelService;

#[async_trait::async_trait]
impl ModelInferenceService for DummyModelService {
    async fn infer(&self, request: &InferenceRequest) -> Result<String, String> {
        println!("[DummyModelService] Received request: {}", request.prompt);
        actix_web::rt::time::sleep(Duration::from_millis(500)).await;
        Ok(format!("This is the mocked completion for: {}", request.prompt))
    }

    fn model_name(&self) -> String {
        "DummyModelService".to_string()
    }

    fn api_base_url(&self) -> String {
        "http://localhost:9999/v1".to_string()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// SSE Helpers
// ═════════════════════════════════════════════════════════════════════════════

fn format_sse_event(event_type: &str, data: &str) -> String {
    let lines: Vec<String> = data.lines().map(|line| format!("data: {}", line)).collect();
    format!("event: {}\n{}\n\n", event_type, lines.join("\n"))
}

fn parse_thinking(text: &str) -> (Option<String>, String) {
    let start_tag = "<thinking>";
    let end_tag = "</thinking>";

    if let Some(start_idx) = text.find(start_tag) {
        if let Some(end_idx) = text.find(end_tag) {
            if end_idx > start_idx {
                let reasoning = text[start_idx + start_tag.len()..end_idx].trim().to_string();
                let before = text[..start_idx].trim();
                let after = text[end_idx + end_tag.len()..].trim();
                let final_answer = if before.is_empty() {
                    after.to_string()
                } else {
                    format!("{}\n{}", before, after).trim().to_string()
                };
                return (Some(reasoning), final_answer);
            }
        }
    }

    (None, text.to_string())
}

fn clean_response(text: &str) -> String {
    // Strip echoed query headers that some models mirror back.
    // Looks for patterns like:
    //   ## User Query
    //   Hello
    //   ### Response:
    //   Greetings!
    let lines: Vec<&str> = text.lines().collect();
    let mut skip = true;
    let mut cleaned: Vec<&str> = Vec::new();

    for line in &lines {
        let trimmed = line.trim();
        if trimmed.starts_with("## User Query")
            || trimmed.starts_with("## User Context")
            || trimmed.starts_with("## Mindmap Data")
            || trimmed.starts_with("## External Instances")
            || trimmed.starts_with("## Image Archive")
            || trimmed.starts_with("### Response:")
            || trimmed.starts_with("**Response:**")
            || trimmed.starts_with("**Answer:**")
        {
            skip = true;
            continue;
        }
        if skip && !trimmed.is_empty() {
            skip = false;
        }
        if !skip {
            cleaned.push(line);
        }
    }

    let result = cleaned.join("\n").trim().to_string();
    if result.is_empty() {
        text.to_string()
    } else {
        result
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Tool Call Parsing & Execution
// ═════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
struct ToolCall {
    name: String,
    input: serde_json::Value,
}

fn parse_tool_calls(text: &str) -> Vec<ToolCall> {
    let mut calls = Vec::new();
    let mut remaining = text;
    loop {
        if let Some(start) = remaining.find("<tool>") {
            let after_start = &remaining[start + 6..];
            if let Some(end) = after_start.find("</tool>") {
                let block = &after_start[..end];
                if let Some(name_start) = block.find("<name>") {
                    let name_after = &block[name_start + 6..];
                    if let Some(name_end) = name_after.find("</name>") {
                        let name = name_after[..name_end].trim().to_string();
                        if let Some(input_start) = block.find("<input>") {
                            let input_after = &block[input_start + 7..];
                            if let Some(input_end) = input_after.find("</input>") {
                                let input_str = input_after[..input_end].trim();
                                let input_json = serde_json::from_str(input_str).unwrap_or(serde_json::Value::Null);
                                calls.push(ToolCall { name, input: input_json });
                            }
                        }
                    }
                }
                remaining = &after_start[end + 7..];
                continue;
            }
        }
        break;
    }
    calls
}

fn strip_tool_blocks(text: &str) -> String {
    let mut result = String::new();
    let mut remaining = text;
    loop {
        if let Some(start) = remaining.find("<tool>") {
            result.push_str(&remaining[..start]);
            if let Some(end) = remaining[start..].find("</tool>") {
                remaining = &remaining[start + end + 7..];
                continue;
            }
        }
        result.push_str(remaining);
        break;
    }
    result.trim().to_string()
}

// ═════════════════════════════════════════════════════════════════════════════
// API Handlers
// ═════════════════════════════════════════════════════════════════════════════

async fn get_about() -> HttpResponse {
    HttpResponse::Ok().json(json!({
        "title": "Avalon",
        "version": "0.2.0",
        "desc": "Context-Aware AI Harness\nRust Backend + Electron Frontend",
        "build": "Rust / Actix-web / reqwest"
    }))
}

async fn list_tools(registry: web::Data<Mutex<tools::ToolRegistry>>, config: web::Data<Mutex<AppConfig>>) -> HttpResponse {
    let reg = registry.lock().unwrap();
    let cfg = config.lock().unwrap();
    let active_set: HashSet<String> = cfg.active_tools.iter().cloned().collect();
    let tools: Vec<serde_json::Value> = reg.list().into_iter().map(|t| {
        let is_active = active_set.contains(&t.name);
        json!({
            "name": t.name,
            "description": t.description,
            "active": is_active,
            "is_core": t.is_core
        })
    }).collect();
    HttpResponse::Ok().json(json!({ "tools": tools }))
}

#[derive(Debug, Deserialize)]
struct PluginsRequest {
    active_tools: Vec<String>,
}

async fn set_plugins(
    body: web::Json<PluginsRequest>,
    config: web::Data<Mutex<AppConfig>>,
) -> HttpResponse {
    let mut cfg = config.lock().unwrap();
    cfg.active_tools = body.active_tools.clone();
    let mut persisted = ConfigStore::load();
    persisted.insert("active_tools".to_string(), json!(&body.active_tools));
    ConfigStore::save(&persisted);
    HttpResponse::Ok().json(json!({ "ok": true }))
}

async fn get_ai_name(config: web::Data<Mutex<AppConfig>>) -> HttpResponse {
    let cfg = config.lock().unwrap();
    HttpResponse::Ok().json(json!({ "ai_name": cfg.ai_name }))
}

#[derive(Debug, Deserialize)]
struct AiNameRequest {
    ai_name: String,
}

async fn set_ai_name(
    body: web::Json<AiNameRequest>,
    config: web::Data<Mutex<AppConfig>>,
) -> HttpResponse {
    let mut cfg = config.lock().unwrap();
    cfg.ai_name = body.ai_name.clone();
    let mut persisted = ConfigStore::load();
    persisted.insert("ai_name".to_string(), json!(&body.ai_name));
    ConfigStore::save(&persisted);
    HttpResponse::Ok().json(json!({ "ok": true }))
}

async fn get_mindmap(
    fs: web::Data<Mutex<FileSystemService>>,
    mindmap: web::Data<Mutex<MindMapService>>,
) -> HttpResponse {
    let fs = fs.lock().unwrap();
    let mut mm = mindmap.lock().unwrap();
    let allowed: Vec<String> = fs.config().allowed_paths.clone();
    mm.build(&allowed, 3);
    HttpResponse::Ok().json(mm.graph())
}

async fn list_models(
    model_service: web::Data<Box<dyn ModelInferenceService>>,
    debug_log: web::Data<Mutex<DebugLog>>,
) -> HttpResponse {
    let svc = match model_service.as_any().downcast_ref::<HttpModelService>() {
        Some(svc) => svc,
        None => {
            return HttpResponse::Ok().json(json!({ "models": ["dummy-model"] }));
        }
    };

    match svc.ollama_list_models().await {
        Ok(models) => HttpResponse::Ok().json(json!({ "models": models })),
        Err(e) => {
            debug_log.lock().unwrap().push("error", json!({ "message": e }));
            let current = svc.model_name();
            HttpResponse::Ok().json(json!({ "models": [current] }))
        }
    }
}

async fn get_model(config: web::Data<Mutex<AppConfig>>) -> HttpResponse {
    let cfg = config.lock().unwrap();
    HttpResponse::Ok().json(json!({ "model": cfg.current_model }))
}

async fn set_model(
    body: web::Json<serde_json::Value>,
    config: web::Data<Mutex<AppConfig>>,
) -> HttpResponse {
    if let Some(model) = body.get("model").and_then(|v| v.as_str()) {
        config.lock().unwrap().current_model = model.to_string();
        let mut persisted = ConfigStore::load();
        persisted.insert("current_model".to_string(), json!(model));
        ConfigStore::save(&persisted);
        HttpResponse::Ok().json(json!({ "ok": true }))
    } else {
        HttpResponse::BadRequest().json(json!({ "error": "Missing model field" }))
    }
}

async fn preload_model(
    query: web::Query<HashMap<String, String>>,
    model_service: web::Data<Box<dyn ModelInferenceService>>,
) -> HttpResponse {
    let model = query.get("model").cloned().unwrap_or_default();
    if model.is_empty() {
        return HttpResponse::BadRequest().json(json!({ "error": "Missing model parameter" }));
    }

    let svc = match model_service.as_any().downcast_ref::<HttpModelService>() {
        Some(svc) => svc,
        None => return HttpResponse::Ok().json(json!({ "ok": true })),
    };

    match svc.ollama_preload(&model).await {
        Ok(()) => HttpResponse::Ok().json(json!({ "ok": true })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e })),
    }
}

fn has_exploratory_intent(text: &str) -> bool {
    let lower = text.to_lowercase();
    let keywords = [
        "research", "learn", "look through", "look at", "look into",
        "explore", "investigate", "study", "analyze", "analyse",
        "understand", "get familiar with", "get to know", "scan",
        "browse", "examine", "review", "survey", "map out",
        "get an overview", "tell me about", "what's in", "what is in",
        "show me around", "walk me through", "give me a tour",
        "how does this work", "how is this structured", "codebase",
        "project structure", "architecture", "overview",
    ];
    keywords.iter().any(|kw| lower.contains(kw))
}

async fn chat_handler(
    query: web::Query<HashMap<String, String>>,
    model_service: web::Data<Box<dyn ModelInferenceService>>,
    debug_log: web::Data<Mutex<DebugLog>>,
    config: web::Data<Mutex<AppConfig>>,
    fs: web::Data<Mutex<FileSystemService>>,
    registry: web::Data<Mutex<tools::ToolRegistry>>,
) -> HttpResponse {
    let message = query.get("message").cloned().unwrap_or_default();
    let selected_model = query.get("model").cloned()
        .or_else(|| Some(config.lock().unwrap().current_model.clone()))
        .unwrap_or_default();

    if message.is_empty() {
        let err = format_sse_event("error", "Missing message parameter");
        return HttpResponse::Ok()
            .content_type("text/event-stream")
            .body(err);
    }

    let ai_name = config.lock().unwrap().ai_name.clone();

    // ── Intent detection: pre-build mind map for exploratory queries ──
    let mindmap_payload = if has_exploratory_intent(&message) {
        debug_log.lock().unwrap().push("mindmap_build", json!({"reason": "exploratory_intent_detected", "query": &message}));
        let fs_svc = fs.lock().unwrap();
        let mut mm = MindMapService::new();
        let allowed: Vec<String> = fs_svc.config().allowed_paths.clone();
        mm.build(&allowed, 3);
        let payload = serde_json::to_value(mm.graph()).unwrap_or(serde_json::Value::Null);
        debug_log.lock().unwrap().push("mindmap_build", json!({"nodes": mm.graph().nodes.len(), "edges": mm.graph().edges.len()}));
        payload
    } else {
        serde_json::Value::Null
    };

    // ── First inference turn ──
    let req = InferenceRequest {
        prompt: message,
        user_context: String::new(),
        mindmap_payload,
        image_archives: Vec::new(),
        other_instances: serde_json::Value::Null,
        model_params: json!({"model": selected_model}),
        ai_name: ai_name.clone(),
    };

    let raw_result = match model_service.infer(&req).await {
        Ok(text) => text,
        Err(e) => {
            let body = format_sse_event("error", &format!("Inference error: {}", e));
            return HttpResponse::Ok()
                .content_type("text/event-stream")
                .body(body);
        }
    };

    let calls = parse_tool_calls(&raw_result);
    let mut sse_events: Vec<String> = Vec::new();

    let (reasoning, final_answer) = if !calls.is_empty() {
        // Emit tool_call events
        for call in &calls {
            let evt = format_sse_event("tool_call", &json!({
                "tool": call.name,
                "input": call.input
            }).to_string());
            sse_events.push(evt);
        }

        // Execute tool calls and build follow-up
        let fs_svc = fs.lock().unwrap();
        let registry = registry.lock().unwrap();
        let cfg = config.lock().unwrap();
        let active_set: HashSet<String> = cfg.active_tools.iter().cloned().collect();
        let mut tool_results: Vec<serde_json::Value> = Vec::new();
        for call in &calls {
            let is_core = registry.get(&call.name).map(|t| t.is_core()).unwrap_or(false);
            if !active_set.contains(&call.name) && !is_core {
                let e = format!("Tool '{}' is deactivated. Activate it in Settings > Plugins to use it.", call.name);
                debug_log.lock().unwrap().push("tool_error", json!({
                    "tool": call.name,
                    "input": call.input,
                    "error": &e
                }));
                let err_evt = format_sse_event("tool_result", &json!({
                    "tool": call.name,
                    "observation": format!("Error: {}", e)
                }).to_string());
                sse_events.push(err_evt);
                tool_results.push(json!({
                    "tool": call.name,
                    "input": call.input,
                    "error": e
                }));
                continue;
            }
            match registry.get(&call.name) {
                Some(tool) => {
                    match tool.execute(call.input.clone(), &tools::ToolContext { fs: &fs_svc, web_fetch: &cfg.web_fetch }).await {
                        Ok(result) => {
                            debug_log.lock().unwrap().push("tool_call", json!({
                                "tool": call.name,
                                "input": call.input,
                                "success": true,
                                "result": &result
                            }));
                            let result_evt = format_sse_event("tool_result", &json!({
                                "tool": call.name,
                                "observation": serde_json::to_string(&result).unwrap_or_default()
                            }).to_string());
                            sse_events.push(result_evt);
                            tool_results.push(json!({
                                "tool": call.name,
                                "input": call.input,
                                "result": result
                            }));
                        }
                        Err(e) => {
                            debug_log.lock().unwrap().push("tool_error", json!({
                                "tool": call.name,
                                "input": call.input,
                                "error": e
                            }));
                            let err_evt = format_sse_event("tool_result", &json!({
                                "tool": call.name,
                                "observation": format!("Error: {}", e)
                            }).to_string());
                            sse_events.push(err_evt);
                            tool_results.push(json!({
                                "tool": call.name,
                                "input": call.input,
                                "error": e
                            }));
                        }
                    }
                }
                None => {
                    let e = format!("Unknown tool: {}", call.name);
                    debug_log.lock().unwrap().push("tool_error", json!({
                        "tool": call.name,
                        "input": call.input,
                        "error": &e
                    }));
                    let err_evt = format_sse_event("tool_result", &json!({
                        "tool": call.name,
                        "observation": format!("Error: {}", e)
                    }).to_string());
                    sse_events.push(err_evt);
                    tool_results.push(json!({
                        "tool": call.name,
                        "input": call.input,
                        "error": e
                    }));
                }
            }
        }
        drop(registry);
        drop(fs_svc);

        let stripped = strip_tool_blocks(&raw_result);
        let follow_up_prompt = format!(
            "{}",
            serde_json::json!({
                "original_response": stripped,
                "tool_results": tool_results,
                "instruction": "Based on the tool results above, provide your final answer to the user. Wrap any step-by-step reasoning in <thinking>...</thinking> tags."
            })
        );

        let follow_up_req = InferenceRequest {
            prompt: follow_up_prompt,
            user_context: String::new(),
            mindmap_payload: serde_json::Value::Null,
            image_archives: Vec::new(),
            other_instances: serde_json::Value::Null,
            model_params: json!({"model": selected_model}),
            ai_name: ai_name.clone(),
        };

        let follow_up_raw = match model_service.infer(&follow_up_req).await {
            Ok(text) => text,
            Err(e) => format!("Error during follow-up: {}", e),
        };

        let (r, ans) = parse_thinking(&follow_up_raw);
        (r, clean_response(&ans))
    } else {
        let (r, ans) = parse_thinking(&raw_result);
        (r, clean_response(&ans))
    };

    // Log
    {
        let mut log = debug_log.lock().unwrap();
        if let Some(ref r) = reasoning {
            log.push("reasoning", json!({ "text": r }));
        }
        log.push("api_response", json!({
            "elapsed_ms": 500,
            "stop_reason": "end_turn",
            "content": [{"type": "text", "text": &final_answer}]
        }));
    }

    // Build SSE stream
    let mut events: Vec<String> = Vec::new();
    events.extend(sse_events);
    if let Some(ref r) = reasoning {
        events.push(format_sse_event("reasoning", r));
    }
    events.push(format_sse_event("text", &final_answer));
    events.push(format_sse_event("done", "1"));

    let stream = stream::iter(events)
        .map(|s| Ok::<_, actix_web::Error>(web::Bytes::from(s)));

    HttpResponse::Ok()
        .content_type("text/event-stream")
        .streaming(stream)
}

async fn get_debug(debug_log: web::Data<Mutex<DebugLog>>) -> HttpResponse {
    let log = debug_log.lock().unwrap();
    HttpResponse::Ok().json(json!({ "log": log.get_all() }))
}

async fn clear_debug(debug_log: web::Data<Mutex<DebugLog>>) -> HttpResponse {
    debug_log.lock().unwrap().clear();
    HttpResponse::Ok().json(json!({ "ok": true }))
}

async fn save_debug(debug_log: web::Data<Mutex<DebugLog>>) -> HttpResponse {
    match debug_log.lock().unwrap().save_to_file() {
        Ok(path) => HttpResponse::Ok().json(json!({ "ok": true, "path": path })),
        Err(e) => HttpResponse::InternalServerError().json(json!({ "error": e })),
    }
}

#[derive(Debug, Deserialize)]
struct PermissionRequest {
    tool: Option<String>,
    allowed: bool,
}

async fn post_permission(
    body: web::Json<PermissionRequest>,
    session_perms: web::Data<Mutex<SessionPermissionManager>>,
    security: web::Data<Mutex<SecurityManager>>,
    debug_log: web::Data<Mutex<DebugLog>>,
) -> HttpResponse {
    let mut perms = session_perms.lock().unwrap();
    let tool = body.tool.clone().unwrap_or_else(|| "unknown".to_string());

    if body.allowed {
        perms.approve(&tool, AccessPermissions::ReadWrite);
        security.lock().unwrap().register_permission(&tool, "*", AccessPermissions::ReadWrite);
        debug_log.lock().unwrap().push("permission_decision", json!({
            "tool": &tool,
            "allowed": true
        }));
        HttpResponse::Ok().json(json!({ "ok": true, "status": "approved" }))
    } else {
        perms.deny(&tool);
        debug_log.lock().unwrap().push("permission_denied", json!({ "tool": &tool }));
        HttpResponse::Ok().json(json!({ "ok": true, "status": "denied" }))
    }
}

async fn get_permissions(
    session_perms: web::Data<Mutex<SessionPermissionManager>>,
) -> HttpResponse {
    let perms = session_perms.lock().unwrap();
    HttpResponse::Ok().json(json!({ "permissions": perms.list() }))
}

async fn revoke_permission(
    path: web::Path<String>,
    session_perms: web::Data<Mutex<SessionPermissionManager>>,
    security: web::Data<Mutex<SecurityManager>>,
    debug_log: web::Data<Mutex<DebugLog>>,
) -> HttpResponse {
    let tool = path.into_inner();
    session_perms.lock().unwrap().revoke(&tool);
    security.lock().unwrap().remove_permission(&tool, "*");
    debug_log.lock().unwrap().push("permission_revoked", json!({ "tool": &tool }));
    HttpResponse::Ok().json(json!({ "ok": true }))
}

// ═════════════════════════════════════════════════════════════════════════════
// File System API Handlers
// ═════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct FsReadRequest {
    path: String,
}

#[derive(Debug, Deserialize)]
struct FsWriteRequest {
    path: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct FsListRequest {
    path: String,
}

#[derive(Debug, Deserialize)]
struct FsDeleteRequest {
    path: String,
}

async fn fs_read(
    body: web::Json<FsReadRequest>,
    fs: web::Data<Mutex<FileSystemService>>,
    debug_log: web::Data<Mutex<DebugLog>>,
) -> HttpResponse {
    let result = fs.lock().unwrap().read_file(&body.path);
    debug_log.lock().unwrap().push("tool_call", json!({
        "tool": "read_file",
        "input": { "path": &body.path },
        "success": result.success
    }));
    if let Some(ref err) = result.error {
        debug_log.lock().unwrap().push("tool_error", json!({ "tool": "read_file", "error": err }));
    }
    HttpResponse::Ok().json(result)
}

async fn fs_write(
    body: web::Json<FsWriteRequest>,
    fs: web::Data<Mutex<FileSystemService>>,
    debug_log: web::Data<Mutex<DebugLog>>,
) -> HttpResponse {
    let result = fs.lock().unwrap().write_file(&body.path, &body.content);
    debug_log.lock().unwrap().push("tool_call", json!({
        "tool": "write_file",
        "input": { "path": &body.path },
        "success": result.success
    }));
    if let Some(ref err) = result.error {
        debug_log.lock().unwrap().push("tool_error", json!({ "tool": "write_file", "error": err }));
    }
    HttpResponse::Ok().json(result)
}

async fn fs_list(
    body: web::Json<FsListRequest>,
    fs: web::Data<Mutex<FileSystemService>>,
    debug_log: web::Data<Mutex<DebugLog>>,
) -> HttpResponse {
    let result = fs.lock().unwrap().list_dir(&body.path);
    debug_log.lock().unwrap().push("tool_call", json!({
        "tool": "list_dir",
        "input": { "path": &body.path },
        "success": result.success
    }));
    if let Some(ref err) = result.error {
        debug_log.lock().unwrap().push("tool_error", json!({ "tool": "list_dir", "error": err }));
    }
    HttpResponse::Ok().json(result)
}

async fn fs_delete(
    body: web::Json<FsDeleteRequest>,
    fs: web::Data<Mutex<FileSystemService>>,
    debug_log: web::Data<Mutex<DebugLog>>,
) -> HttpResponse {
    let result = fs.lock().unwrap().delete_file(&body.path);
    debug_log.lock().unwrap().push("tool_call", json!({
        "tool": "delete_file",
        "input": { "path": &body.path },
        "success": result.success
    }));
    if let Some(ref err) = result.error {
        debug_log.lock().unwrap().push("tool_error", json!({ "tool": "delete_file", "error": err }));
    }
    HttpResponse::Ok().json(result)
}

async fn fs_config_get(
    fs: web::Data<Mutex<FileSystemService>>,
) -> HttpResponse {
    let cfg = fs.lock().unwrap().config().clone();
    HttpResponse::Ok().json(cfg)
}

async fn fs_config_post(
    body: web::Json<FileSystemConfig>,
    fs: web::Data<Mutex<FileSystemService>>,
) -> HttpResponse {
    if let Err(e) = body.save() {
        return HttpResponse::InternalServerError().json(json!({ "error": e }));
    }
    fs.lock().unwrap().reload_config();
    HttpResponse::Ok().json(json!({ "ok": true }))
}

async fn web_config_get(config: web::Data<Mutex<AppConfig>>) -> HttpResponse {
    let cfg = config.lock().unwrap();
    HttpResponse::Ok().json(&cfg.web_fetch)
}

async fn web_config_post(
    body: web::Json<WebFetchConfig>,
    config: web::Data<Mutex<AppConfig>>,
) -> HttpResponse {
    let mut cfg = config.lock().unwrap();
    cfg.web_fetch = body.into_inner();
    if let Err(e) = cfg.web_fetch.save() {
        return HttpResponse::InternalServerError().json(json!({ "error": e }));
    }
    HttpResponse::Ok().json(json!({ "ok": true }))
}

async fn inference_handler(
    req: web::Json<InferenceRequest>,
    model_service: web::Data<Box<dyn ModelInferenceService>>,
    security: web::Data<Mutex<SecurityManager>>,
) -> HttpResponse {
    let access_req = FileAccessRequest {
        action: "read".to_string(),
        path: "src/main.rs".to_string(),
        calling_module: "core_system".to_string(),
        owner: model_service.model_name(),
    };

    let allowed = {
        let sm = security.lock().unwrap();
        sm.check_access(&access_req).unwrap_or(false)
    };

    if !allowed {
        return HttpResponse::Unauthorized().json(json!({
            "error": "Security check failed: access denied."
        }));
    }

    let completion_result = match model_service.infer(&req).await {
        Ok(completion) => completion,
        Err(e) => {
            return HttpResponse::InternalServerError().json(json!({ "error": e }));
        }
    };

    HttpResponse::Ok().json(InferenceResponse {
        completion: completion_result,
        model_used: model_service.model_name(),
    })
}

// ═════════════════════════════════════════════════════════════════════════════
// Main
// ═════════════════════════════════════════════════════════════════════════════

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let mut registry = tools::ToolRegistry::new();
    registry.register(Box::new(tools::fs_tools::ReadFileTool));
    registry.register(Box::new(tools::fs_tools::WriteFileTool));
    registry.register(Box::new(tools::fs_tools::ListDirTool));
    registry.register(Box::new(tools::fs_tools::DeleteFileTool));
    registry.register(Box::new(tools::config_tool::GetFsConfigTool));
    registry.register(Box::new(tools::mindmap_tool::MindMapTool));
    registry.register(Box::new(tools::fetch_tool::FetchUrlTool));
    registry.register(Box::new(tools::remote_mindmap_tool::RemoteMindMapTool));
    registry.register(Box::new(tools::web_scrape_tool::WebScrapeTool));

    let all_tool_names: Vec<String> = registry.names().iter().map(|s| s.to_string()).collect();
    let mut config = AppConfig::new();
    // Migrate old tool names
    config.active_tools = config.active_tools.iter().map(|name| {
        if name == "build_mindmap" { "mindmap".to_string() } else { name.clone() }
    }).collect();
    // Ensure core tools are always active
    let core_names: Vec<String> = registry.list().iter().filter(|t| t.is_core).map(|t| t.name.clone()).collect();
    let active_set: std::collections::HashSet<String> = config.active_tools.iter().cloned().collect();
    for core_name in core_names {
        if !active_set.contains(&core_name) {
            config.active_tools.push(core_name);
        }
    }
    if config.active_tools.is_empty() {
        config.active_tools = all_tool_names.clone();
    }
    let active_tools_set: std::collections::HashSet<String> = config.active_tools.iter().cloned().collect();
    let active_tools_list: Vec<&str> = all_tool_names.iter().filter(|name| active_tools_set.contains(*name)).map(|s| s.as_str()).collect();
    let tools_list = active_tools_list.join(", ");
    let tools_description = format!("Available tools: {}.\n\nUse get_fs_config to read the current file system limiter rules so you can explain to the user exactly which paths are allowed or denied. The config file itself (.avalon_fs.json) is always readable for transparency. All file operations are gated by the FileSystem Limiter. If a path is blocked, explain why using the current rules rather than asking the user to manually edit the config.\n\nUse fetch_url to download content from a public URL. Supports text and images (returned as base64). Respects domain allow-lists, size limits, and timeouts configured in Settings > Web Fetch. Only activate this tool if the user explicitly asks you to read a remote file or image.\n\nUse remote_mindmap to download an entire public GitHub repository as a zip, build a structural graph from it, merge it with the local mindmap, then delete the temporary download. Max 25 MB. This is useful for comparing your local project with an open-source dependency or reference implementation.

Use web_scrape to recursively scrape a website starting from a URL, extracting text and image references up to the configured max depth. Respects robots.txt, rate limits, and domain restrictions.\n\nWhen the user asks you to research, learn, look through, explore, investigate, study, analyze, understand, get familiar with, scan, browse, examine, review, survey, map out, or get an overview of a codebase or project, you should FIRST use mindmap to get a structural understanding of the files and their relationships. Then read the most relevant files before answering.", tools_list);

    let model_service: Box<dyn ModelInferenceService> = match HttpModelService::new() {
        Ok(mut svc) => {
            svc.tools_description = tools_description;
            println!("[Avalon] HttpModelService initialized (model: {})", svc.model_name());
            Box::new(svc)
        }
        Err(e) => {
            println!("[Avalon] Warning: {} Falling back to DummyModelService.", e);
            Box::new(DummyModelService)
        }
    };

    let model_service_data = web::Data::new(model_service);
    let security_data = web::Data::new(Mutex::new(SecurityManager::new()));
    let debug_log_data = web::Data::new(Mutex::new(DebugLog::new()));
    let session_perms_data = web::Data::new(Mutex::new(SessionPermissionManager::new()));
    let config_data = web::Data::new(Mutex::new(config));
    let fs_data = web::Data::new(Mutex::new(FileSystemService::new()));
    let registry_data = web::Data::new(Mutex::new(registry));
    let mindmap_data = web::Data::new(Mutex::new(MindMapService::new()));

    println!("Starting the Avalon Backend API Server on http://127.0.0.1:8080");

    HttpServer::new(move || {
        let cors = Cors::default()
            .allowed_origin_fn(|origin, _req_head| {
                let s = origin.to_str().unwrap_or("");
                s.starts_with("http://127.0.0.1")
                    || s.starts_with("http://localhost")
                    || s.starts_with("file://")
                    || s == "null"
            })
            .allow_any_method()
            .allow_any_header()
            .max_age(3600);

        App::new()
            .wrap(cors)
            .app_data(model_service_data.clone())
            .app_data(security_data.clone())
            .app_data(debug_log_data.clone())
            .app_data(session_perms_data.clone())
            .app_data(config_data.clone())
            .app_data(fs_data.clone())
            .app_data(registry_data.clone())
            .app_data(mindmap_data.clone())
            // Legacy endpoint
            .service(web::resource("/v1/infer").route(web::post().to(inference_handler)))
            // GUI endpoints
            .service(web::resource("/api/models").route(web::get().to(list_models)))
            .service(
                web::resource("/api/model")
                    .route(web::get().to(get_model))
                    .route(web::post().to(set_model))
            )
            .service(web::resource("/api/preload").route(web::get().to(preload_model)))
            .service(web::resource("/api/chat").route(web::get().to(chat_handler)))
            .service(web::resource("/api/debug").route(web::get().to(get_debug)))
            .service(web::resource("/api/debug/clear").route(web::post().to(clear_debug)))
            .service(web::resource("/api/debug/save").route(web::post().to(save_debug)))
            .service(web::resource("/api/permission").route(web::post().to(post_permission)))
            .service(web::resource("/api/permissions").route(web::get().to(get_permissions)))
            .service(web::resource("/api/permissions/{tool}").route(web::delete().to(revoke_permission)))
            .service(web::resource("/api/about").route(web::get().to(get_about)))
            .service(web::resource("/api/tools").route(web::get().to(list_tools)))
            .service(web::resource("/api/plugins").route(web::post().to(set_plugins)))
            .service(
                web::resource("/api/ai_name")
                    .route(web::get().to(get_ai_name))
                    .route(web::post().to(set_ai_name))
            )
            .service(web::resource("/api/mindmap").route(web::get().to(get_mindmap)))
            // File system endpoints
            .service(web::resource("/api/fs/read").route(web::post().to(fs_read)))
            .service(web::resource("/api/fs/write").route(web::post().to(fs_write)))
            .service(web::resource("/api/fs/list").route(web::post().to(fs_list)))
            .service(web::resource("/api/fs/delete").route(web::post().to(fs_delete)))
            .service(
                web::resource("/api/fs/config")
                    .route(web::get().to(fs_config_get))
                    .route(web::post().to(fs_config_post))
            )
            .service(
                web::resource("/api/web/config")
                    .route(web::get().to(web_config_get))
                    .route(web::post().to(web_config_post))
            )
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
