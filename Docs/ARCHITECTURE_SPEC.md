# Avalon Architecture Specification

**Last Updated:** 2026-04-28
**Status:** Active development
**Primary Goal:** A secure, local-first AI coding harness with plugin-based tool execution.

---

## 1. Architectural Pillars

Avalon uses a **local micro-orchestrator** pattern: a Rust backend serves an Electron frontend over a local HTTP API. No external services are required except the LLM itself (local via Ollama or cloud via OpenAI-compatible API).

**Stack:**

| Layer | Technology | Responsibility |
|-------|-----------|----------------|
| Backend | Rust / Actix-web / reqwest | Model inference, tool execution, file system gating, permissions, debug logging, mind map generation |
| Frontend | Electron / Vanilla JS | Chat UI, settings panel, permission dialogs, SSE streaming, mind map viewer |
| Model | Ollama (local) or OpenAI API | LLM inference |
| Storage | `.avalon_fs.json`, `.avalon_state.json`, `logs/` | Persistent config and debug logs |

**Communication:** JSON over HTTP. Server-Sent Events (SSE) for real-time chat streaming.

---

## 2. Security & Access Control

Security is layered across multiple subsystems:

### 2.1 File System Limiter
- Config: `.avalon_fs.json` with `default_policy`, `allowed_paths`, `denied_paths`, `max_file_size`
- Rule: deny list wins > allow list > default policy
- All file tools (`read_file`, `write_file`, `list_dir`, `delete_file`) enforce this

### 2.2 Session Permission Manager
- User-driven, session-scoped approvals for write/delete operations
- Tools can be approved, denied, or revoked per session
- Frontend renders approval dialogs dynamically

### 2.3 Web Fetch Security
- Config: `.avalon_state.json` under `web_fetch`
- Layered protections: scheme blocking, SSRF private-IP blocking, DNS rebind checks, domain allow/block lists, content-type guards, size limits, timeouts, rate limiting, robots.txt respect
- Never executes downloaded content; only parses as text or image

### 2.4 Security Manager (Legacy)
- Module-level `ReadOnly` / `WriteOnly` / `ReadWrite` / `None` permissions
- Used by the legacy `/v1/infer` endpoint

Full details: `SECURITY_PROTOCOL.md`

---

## 3. Core Contracts & Data Flow

### Primary Endpoint: `GET /api/chat`

SSE streaming endpoint. Accepts `message`, `model`, and `history` query parameters.

#### InferenceRequest (internal struct)

```rust
pub struct InferenceRequest {
    pub prompt: String,
    pub user_context: String,
    pub mindmap_payload: Value,
    pub image_archives: Vec<Value>,
    pub other_instances: Value,
    pub model_params: Value,
    pub ai_name: String,
}
```

#### SSE Event Types

| Event | Description |
|-------|-------------|
| `reasoning` | Step-by-step thinking from `<thinking>` tags |
| `text` | Final answer text |
| `tool_call` | A tool was invoked |
| `tool_result` | Result of a tool execution |
| `permission` | User approval is needed |
| `error` | Backend or connection error |
| `done` | Turn completed, includes iteration count |

---

## 4. Model Orchestrator

The `ModelInferenceService` trait is the critical decoupling point. Two implementations exist:

1. **HttpModelService** — connects to any OpenAI-compatible API or Ollama
2. **DummyModelService** — fallback for testing when no model is available

### Orchestration Flow

1. **Intent Detection** — checks if the query is exploratory (research, explore, investigate, etc.) and pre-builds the mind map
2. **Prompt Construction** — combines user query, context, mind map data, image archives, and external instances
3. **Inference** — sends the structured prompt to the LLM
4. **Tool Call Parsing** — extracts `<tool>...</tool>` blocks from the response
5. **Tool Execution** — runs each tool via the `ToolRegistry`, streaming results back
6. **Follow-up Inference** — sends tool results back to the LLM for a final answer
7. **Output Sanitization** — strips echoed headers, extracts `<thinking>` tags, emits SSE events

---

## 5. Plugin Architecture

Tools implement the `Tool` trait and register in a `ToolRegistry`:

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn is_core(&self) -> bool { true }
    async fn execute(&self, input: Value, ctx: &ToolContext<'_>
    ) -> Result<Value, String>;
}
```

**Core tools** (always active, hidden from plugin list):
- `read_file`, `write_file`, `list_dir`, `delete_file`, `get_fs_config`

**Optional plugins** (user-activatable):
- `mindmap` — scans codebase and builds relationship graph
- `fetch_url` — downloads content from URLs (text, images, PDFs)
- `remote_mindmap` — downloads GitHub repos as zip, builds mind map, merges, deletes temp
- `web_scrape` — recursively scrapes websites with BFS, robots.txt, rate limits

---

## 6. Mind Map (Codebase Graph)

A structural understanding layer that scans allowed paths and builds a graph of files, directories, and their relationships.

### Process
1. **Scan** — recursively walks allowed paths up to depth 3
2. **Parse** — extracts imports from Rust (`use`, `mod`), JS/TS (`import`, `require`), Python (`import`, `from`)
3. **Build** — creates nodes (files/dirs) and edges (imports/contains)
4. **Inject** — sends the graph to the AI as context before answering

### Intent Detection
Exploratory keywords trigger automatic mind map building:
- `research`, `explore`, `investigate`, `study`, `analyze`
- `understand`, `look through`, `scan`, `review`
- `codebase`, `project structure`, `architecture`, `overview`

---

## 7. Config Files

| File | Purpose |
|------|---------|
| `.avalon_fs.json` | File system limiter rules |
| `.avalon_state.json` | App state (model, active tools, AI name, web fetch config) |
| `logs/avalon-debug-*.md` | Saved debug logs |

---

## 8. API Surface

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
| POST | `/api/fs/read` | Read a file |
| POST | `/api/fs/write` | Write a file |
| POST | `/api/fs/list` | List a directory |
| POST | `/api/fs/delete` | Delete a file/directory |
| POST | `/v1/infer` | Legacy inference endpoint |

---

## 9. Future Directions

- **Multi-model sessions** — switch models mid-conversation
- **Code execution sandbox** — safe, isolated execution of generated code
- **Git integration** — diff, commit, and branch operations via tools
- **Persistent memory** — long-term knowledge across sessions
