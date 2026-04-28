# Avalon — Capabilities & Functions

## Overview

Avalon is a context-aware AI coding assistant that bridges a local language model (via Ollama or OpenAI-compatible API) with a local file system. It consists of a Rust backend (Actix-web) serving an Electron frontend, communicating over a local HTTP API at `127.0.0.1:8080`.

---

## Architecture

| Layer | Technology | Responsibility |
|-------|-----------|----------------|
| Backend | Rust / Actix-web / reqwest | Model inference, tool execution, file system gating, permissions, debug logging, mind map generation |
| Frontend | Electron / Vanilla JS | Chat UI, settings panel, permission dialogs, SSE streaming, mind map viewer |
| Model | Ollama (local) or OpenAI API | LLM inference |
| Storage | `.avalon_fs.json`, `.avalon_state.json`, `logs/` | Persistent config and debug logs |

---

## Core Capabilities

### 1. Model Inference

The backend can connect to any OpenAI-compatible API or Ollama:
- **List models**: `/api/models` — queries Ollama's `/api/tags`
- **Select model**: `/api/model` (GET/POST) — persists to `.avalon_state.json`
- **Preload model**: `/api/preload?model=` — keeps a model warm in Ollama memory
- **Chat**: `/api/chat?message=` — primary SSE streaming endpoint for conversation

Environment variables:
- `AVALON_MODEL_NAME` — default model
- `AVALON_MODEL_API_BASE` — e.g. `http://localhost:11434` or `https://api.openai.com/v1`
- `AVALON_MODEL_API_KEY` — for cloud APIs

### 2. Tool System (Plugin Architecture)

Tools are now **plugins**. They implement the `Tool` trait, register in a `ToolRegistry`, and the system prompt dynamically lists them. Adding a new tool requires only:
1. Implementing `Tool` in `src/tools/<name>.rs`
2. Calling `registry.register(Box::new(MyTool))` in `main.rs`

#### Core Tools (always active)

| Tool | Description | Arguments |
|------|-------------|-----------|
| `read_file` | Reads file contents | `{ path: string }` |
| `write_file` | Writes or overwrites a file | `{ path: string, content: string }` |
| `list_dir` | Lists files and directories | `{ path: string }` |
| `delete_file` | Deletes a file or directory | `{ path: string }` |
| `get_fs_config` | Reads the file system limiter config | `{}` |

#### Optional Plugins

| Tool | Description | Arguments |
|------|-------------|-----------|
| `mindmap` | Scans allowed paths and builds a graph of files and their relationships | `{}` |
| `fetch_url` | Downloads content from any public URL. Supports text, images (base64), and PDFs (text extracted). Respects domain lists, size limits, and timeouts from Web Fetch config. | `{ url: string }` |
| `remote_mindmap` | Downloads a public GitHub repo as a zip, builds a mind map, merges it with the local graph, then deletes the temp download. Max 25 MB. | `{ url: string }` |
| `web_scrape` | Recursively scrapes a website starting from a URL. Extracts text and image references, follows links up to max depth. Respects robots.txt, rate limits, and domain restrictions. | `{ url: string, max_depth?: number }` |

Tool calls are embedded in AI responses as XML:
```xml
<tool>
  <name>read_file</name>
  <input>{"path": "src/main.rs"}</input>
</tool>
```

The backend parses these, executes the matching plugin, and sends results back to the model for a follow-up inference turn.

### 3. File System Limiter

All file operations are gated by `FileSystemConfig`, loaded from `.avalon_fs.json`:

```json
{
  "default_policy": "deny",
  "allowed_paths": ["D:/Projects", "D:/Avalon/src"],
  "denied_paths": ["C:/", "D:/Secrets"],
  "max_file_size": 10485760
}
```

Rules:
- **Deny list wins** — a path in `denied_paths` is always blocked
- **Allow list** — if not empty, only matched paths are permitted
- **Default policy** — `allow` or `deny`, used when allow list is empty
- **Max file size** — blocks reads of files exceeding the limit (stored in bytes, displayed in MB)
- **Config transparency** — `.avalon_fs.json` is always readable so the AI can explain rules

### 4. Permission System

Some tool executions (write/delete) trigger a user approval dialog:
- **Approve**: grants the tool `ReadWrite` for the session
- **Deny**: blocks it
- **Revoke**: `/api/permissions/{tool}` (DELETE) — removes approval
- **Active permissions**: `/api/permissions` (GET) — lists granted tools with timestamps

The permission UI is dynamically populated with tool descriptions fetched from `/api/tools`.

### 5. Security Manager

A backend enforcement layer (`SecurityManager`) that tracks module-level permissions:
- Each module can have specific `ReadOnly`, `WriteOnly`, `ReadWrite`, or `None` access per path
- Currently used by the legacy `/v1/infer` endpoint

### 6. Debug Logging

A comprehensive debug log captures every internal event:
- Session start/end
- Iterations
- LLM requests and responses
- Tool calls and results
- Permission requests, approvals, denials, revocations
- Mind map builds and intent detection
- Errors

Endpoints:
- `GET /api/debug` — returns all log entries
- `POST /api/debug/clear` — wipes the log
- `POST /api/debug/save` — writes a Markdown debug log to `logs/avalon-debug-{timestamp}.md`

The frontend polls `/api/debug` every 100ms and renders events with color coding.

### 7. SSE Streaming

The chat endpoint streams events to the frontend in real time:

| Event | Description |
|-------|-------------|
| `reasoning` | Step-by-step thinking extracted from `<thinking>` tags |
| `text` | Final answer text |
| `tool_call` | A tool was invoked |
| `tool_result` | Result of a tool execution |
| `permission` | User approval is needed |
| `error` | Backend or connection error |
| `done` | Turn completed, includes iteration count |

### 8. Settings Panel

Collapsible sections in the frontend:
- **Model** — current model display
- **AI Assistant** — name the AI calls itself in conversation
- **Active Session Permissions** — revoke granted tools
- **About Avalon** — version, description, build info
- **File System Limiter** — default policy, max file size (MB), allowed/denied path lists
- **Web Fetch** — max depth, confirm unknown domains, timeout, max size, respect robots.txt, allowed/blocked domain lists
- **Plugins** — activate/deactivate tools, save changes

Paths can be added/removed and are saved to `.avalon_fs.json` immediately.

### 9. Electron Lifecycle

The Electron app (`client/main.js`):
- **Starts** the Rust backend automatically when the app opens
- **Kills** the backend automatically when the app quits
- Loads `client/ui/index.html` in a `1400x900` window
- Supports `--dev` flag to open DevTools

---

## Mind Map (Codebase Graph)

### What It Is

The Mind Map is a structural understanding layer that scans your codebase and builds a graph of files, directories, and their relationships (imports, references, directory containment). This gives the AI context about *how* your project is organized before it answers questions.

### How It Works

1. **Scan** — recursively walks allowed paths up to depth 3
2. **Parse** — extracts import/references from:
   - **Rust**: `use`, `mod`
   - **JavaScript/TypeScript**: `import ... from`, `require(...)`
   - **Python**: `import`, `from ... import`
3. **Build** — creates a graph with nodes (files/dirs) and edges (imports/contains)
4. **Inject** — sends the graph to the AI as `## Mindmap Data` context

### Intent Detection (Automatic Mind Map)

When you use exploratory language, Avalon automatically builds and injects the mind map *before* the AI even responds. No need to ask for it explicitly.

**Detected keywords** (case-insensitive):
- `research`, `learn`, `look through`, `look at`, `look into`
- `explore`, `investigate`, `study`, `analyze`, `analyse`
- `understand`, `get familiar with`, `get to know`
- `scan`, `browse`, `examine`, `review`, `survey`
- `map out`, `get an overview`, `tell me about`
- `what's in`, `what is in`, `show me around`
- `walk me through`, `give me a tour`
- `how does this work`, `how is this structured`
- `codebase`, `project structure`, `architecture`, `overview`

Example:
> "Research my codebase and tell me how it's structured."

Avalon will:
1. Detect `research` + `structured` → build mind map
2. Inject the graph into the AI's context
3. The AI answers with actual knowledge of your file layout

### Manual Mind Map

You can also trigger it manually:
- **Frontend**: Click the "Mindmap" button in the debug panel
- **AI tool**: Ask the AI to `<tool><name>mindmap</name><input>{}</input></tool>`
- **API**: `GET /api/mindmap` returns the raw graph JSON

### Graph Format

```json
{
  "nodes": [
    { "id": "D:/Avalon/src/main.rs", "label": "main.rs", "node_type": "file", "metadata": {} },
    { "id": "D:/Avalon/src/fs.rs", "label": "fs.rs", "node_type": "file", "metadata": {} }
  ],
  "edges": [
    { "source": "D:/Avalon/src/main.rs", "target": "D:/Avalon/src/fs.rs", "relation": "imports" }
  ],
  "root": "D:/Avalon/src"
}
```

Node types: `file`, `dir`, `symbol`
Edge relations: `imports`, `references`, `contains`, `depends_on`

---

## AI Assistant Naming

Avalon (the harness) and the AI assistant have separate identities. You can rename the AI through Settings:
- **Default**: "Avalon"
- **Stored**: `.avalon_state.json` under `ai_name`
- **API**: `GET/POST /api/ai_name`
- **Effect**: Changes how the AI introduces itself in the system prompt

Example names: "Merlin", "Friday", "JARVIS", "Cortana", "HAL"

---

## Plugin Activation

Tools can be activated or deactivated per-session:
- **Settings > Plugins** — checkbox list with descriptions
- **Save** — persists to `.avalon_state.json` under `active_tools`
- **Effect** — deactivated tools are blocked at execution time and excluded from the system prompt
- **Restart** — required for the system prompt to fully update with new tool lists

---

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/models` | List available models |
| GET/POST | `/api/model` | Get/set current model |
| GET | `/api/preload` | Preload a model in Ollama |
| GET | `/api/chat` | SSE chat stream |
| GET | `/api/debug` | Get debug log |
| POST | `/api/debug/clear` | Clear debug log |
| POST | `/api/debug/save` | Save debug log to file |
| POST | `/api/permission` | Approve/deny a tool |
| GET | `/api/permissions` | List active permissions |
| DELETE | `/api/permissions/{tool}` | Revoke a permission |
| GET | `/api/about` | App metadata |
| GET | `/api/tools` | List all registered tools |
| POST | `/api/plugins` | Set active tools |
| GET/POST | `/api/ai_name` | Get/set AI assistant name |
| GET | `/api/mindmap` | Get the codebase graph |
| GET/POST | `/api/fs/config` | Get/set file system limiter config |
| GET/POST | `/api/web/config` | Get/set web fetch config |
| POST | `/api/fetch` | Direct fetch URL (bypasses AI tool gatekeeping) |
| POST | `/api/fs/read` | Read a file |
| POST | `/api/fs/write` | Write a file |
| POST | `/api/fs/list` | List a directory |
| POST | `/api/fs/delete` | Delete a file/directory |
| POST | `/v1/infer` | Legacy inference endpoint |

### Direct Fetch (`/api/fetch`)

Bypasses the AI tool-calling gatekeeping and runs the same safe fetch pipeline directly. Returns sanitized content for user review before it is sent to the model.

**Request:**
```json
POST /api/fetch
{ "url": "https://example.com/document.pdf" }
```

**Response (text):**
```json
{
  "url": "https://example.com/document.pdf",
  "type": "text",
  "mime_type": "text/html",
  "size": 528,
  "content": "..."
}
```

**Response (image):**
```json
{
  "url": "https://example.com/image.png",
  "type": "image",
  "mime_type": "image/png",
  "size": 5969,
  "base64": "iVBORw0KGgo..."
}
```

**Response (PDF):**
```json
{
  "url": "https://example.com/document.pdf",
  "type": "pdf",
  "mime_type": "application/pdf",
  "size": 13248,
  "content": "Extracted plain text..."
}
```

Security is identical to the `fetch_url` tool — all URL validation, SSRF blocking, content sanitization, size limits, and domain checks are enforced in the Rust backend.

---

## Web Fetch

### Config

The Web Fetch config is stored in `.avalon_state.json` under the `web_fetch` key:

```json
{
  "max_depth": 1,
  "confirm_domains": true,
  "allowed_domains": ["github.com", "raw.githubusercontent.com", "gist.github.com", "api.github.com"],
  "blocked_domains": [],
  "timeout_secs": 10,
  "max_size_mb": 5,
  "respect_robots_txt": true,
  "rate_limit_ms": 1000
}
```

| Field | Description |
|-------|-------------|
| `max_depth` | How many levels deep `web_scrape` follows links (1–10) |
| `confirm_domains` | If true, unknown domains require explicit approval via the allowed list |
| `allowed_domains` | Domains that are always permitted |
| `blocked_domains` | Domains that are always denied |
| `timeout_secs` | Request timeout in seconds (1–120) |
| `max_size_mb` | Max response size in megabytes (1–100) |
| `respect_robots_txt` | Whether `web_scrape` respects `robots.txt` rules |
| `rate_limit_ms` | Minimum milliseconds between requests to the same domain |

### Security Model

Layered protections enforced in the tool layer:

| Layer | Implementation |
|-------|----------------|
| URL scheme | Block `file://`, `javascript:`, `data:`, `ftp://` |
| SSRF | Block private IPs (10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16, 127.0.0.0/8, etc.) |
| DNS rebind | Resolve hostname, check IP before request |
| Domain gate | Allow-list + block-list + confirmation dialog |
| Content | Content-type whitelist: `text/*`, `image/*`, `application/json`, `application/xml` |
| HTML | Sanitize: remove scripts, styles, iframes, event handlers |
| Size | Enforce `max_size_mb` per response |
| Timeout | Enforce `timeout_secs` per request |
| Rate | Per-domain cooldown (`rate_limit_ms`) |
| Robots | Fetch and respect `robots.txt` |
| No execution | Never run downloaded content; only parse as text/image |

---

## Extending Avalon

### Add a New Tool Plugin

Create `src/tools/my_tool.rs`:

```rust
use serde_json;
use crate::tools::{Tool, ToolContext};

pub struct MyTool;

#[async_trait::async_trait]
impl Tool for MyTool {
    fn name(&self) -> &str { "my_tool" }
    fn description(&self) -> &str { "Does something useful." }
    async fn execute(&self, input: serde_json::Value, ctx: &ToolContext<'_>) -> Result<serde_json::Value, String> {
        // implementation
    }
}
```

Register it in `src/main.rs`:
```rust
registry.register(Box::new(tools::my_tool::MyTool));
```

The tool immediately appears in:
- The system prompt (`/api/chat`)
- The tool discovery endpoint (`/api/tools`)
- The permission dialog (frontend fetches descriptions dynamically)

---

## Files & Config

| File | Purpose |
|------|---------|
| `src/main.rs` | Backend entry point, HTTP routes, chat orchestration, intent detection |
| `src/fs.rs` | File system service, limiter config, path normalization |
| `src/mindmap.rs` | Mind map graph builder, import parser, graph resolution |
| `src/tools/mod.rs` | Tool trait, registry, context |
| `src/tools/fs_tools.rs` | File operation tool plugins |
| `src/tools/config_tool.rs` | Config reading tool plugin |
| `src/tools/mindmap_tool.rs` | Mind map building tool plugin |
| `src/tools/fetch_tool.rs` | URL fetching tool plugin |
| `src/tools/remote_mindmap_tool.rs` | GitHub repo download and mindmap merge plugin |
| `src/tools/web_scrape_tool.rs` | Recursive web scraping plugin |
| `client/main.js` | Electron bootstrap, backend lifecycle |
| `client/ui/app.js` | Frontend app logic, SSE, settings, permissions, mind map viewer |
| `client/ui/style.css` | Frontend styling |
| `client/ui/index.html` | Frontend markup |
| `.avalon_fs.json` | Persistent file system limiter rules |
| `.avalon_state.json` | Persistent app state (current model, active tools, AI name, web fetch config) |
| `logs/avalon-debug-*.md` | Saved debug logs |
