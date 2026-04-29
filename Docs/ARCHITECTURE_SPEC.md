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
| Storage | `.avalon_fs.json`, `.avalon_state.json`, `logs/`, `.avalon.db` | Persistent config, debug logs, SQLite vault |

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
- `vault_search` / `vault_read` — query and retrieve MindVault documents
- `vision_search` / `vision_read` — query and retrieve VisionVault images
- `dispatch_agent` — queue an agent task
- `board_post` / `board_read` — inter-agent messaging

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
| `logs/debug/` | Saved debug logs (chat history + events) |

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

## 6. Vault Services

### 6.1 MindVault (`src/vault.rs`)

- **Storage**: SQLite `vault_documents` table + `vault_fts` FTS5 virtual table
- **Auto-ingest**: File writes, URL fetches, and web scrapes automatically commit text content
- **Deduplication**: SHA-256 hash prevents duplicate ingestion
- **Sanitization pipeline**: null-byte removal, control-character stripping, whitespace normalization, HTML tag stripping

### 6.2 VisionVault (`src/vision.rs`)

- **Storage**: SQLite `vision_images` table + `vision_fts` FTS5 virtual table
- **Auto-ingest**: Image reads via `/api/fs/image` automatically store metadata
- **Format detection**: Magic-byte-based identification (PNG, JPEG, GIF, WebP, BMP)
- **Dimension extraction**: Header parsing without external dependencies
- **Confirmation workflow**: Images ingested with `confirmed = 0`; user confirms via API

### 6.3 Agent System (`src/agents.rs`)

- **Registry**: SQLite `agents` table with whitelisted `allowed_tools`
- **Security**: Forbidden tools (`bash`, `shell`, `exec`, `eval`, `create_agent`, `delete_agent`, `update_agent`) rejected at creation time
- **Dispatch**: `agent_dispatches` table tracks status lifecycle
- **Board**: `agent_board` table for per-dispatch messaging
- **Memory**: `agent_memory` table for session summaries

---

## 7. Database Schema

Single SQLite file `.avalon.db` with the following tables:

| Table | Purpose |
|-------|---------|
| `vault_documents` | MindVault documents |
| `vault_fts` | FTS5 virtual table for document search |
| `vision_images` | VisionVault image metadata |
| `vision_fts` | FTS5 virtual table for image search |
| `agents` | Agent definitions |
| `agent_dispatches` | Task dispatch records |
| `agent_board` | Inter-agent messages |
| `agent_memory` | Session summaries |

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
| GET/POST | `/api/security/config` | Get/set security config |
| GET/POST | `/api/audit/config` | Get/set audit config |
| POST | `/api/fs/read` | Read a file |
| POST | `/api/fs/write` | Write a file |
| POST | `/api/fs/list` | List a directory |
| POST | `/api/fs/delete` | Delete a file/directory |
| POST | `/api/fs/image` | Read an image file |
| POST | `/api/fetch` | Direct fetch URL |
| POST | `/api/spellcheck` | Spell check |
| GET | `/api/vault/search` | Search MindVault |
| GET | `/api/vault/document/{id}` | Get MindVault document |
| DELETE | `/api/vault/document/{id}` | Delete MindVault document |
| GET | `/api/vision/search` | Search VisionVault |
| GET | `/api/vision/image/{id}` | Get VisionVault image |
| POST | `/api/vision/confirm/{id}` | Confirm image description |
| DELETE | `/api/vision/image/{id}` | Delete VisionVault image |
| GET | `/api/agents` | List agents |
| POST | `/api/agents` | Create agent |
| GET | `/api/agents/{name}` | Get agent |
| POST | `/api/agents/{name}` | Update agent |
| DELETE | `/api/agents/{name}` | Delete agent |
| POST | `/api/agents/dispatch` | Dispatch agent |
| GET | `/api/agents/dispatch/{id}` | Get dispatch |
| POST | `/api/agents/dispatch/{id}/cancel` | Cancel dispatch |
| GET | `/api/agents/dispatch/{id}/board` | Read board |
| POST | `/api/agents/dispatch/{id}/board` | Post to board |
| POST | `/v1/infer` | Legacy inference endpoint |

---

## 9. Future Directions

- **Multi-model sessions** — switch models mid-conversation
- **Code execution sandbox** — safe, isolated execution of generated code
- **Git integration** — diff, commit, and branch operations via tools
- **Persistent memory** — long-term knowledge across sessions
- **Agent execution** — full in-loop agent executor with model inference
- **Image generation workers** — integrate Stable Diffusion or DALL-E via `AgentWorker` trait
