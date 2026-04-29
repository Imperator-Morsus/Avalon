# Changelog

## 0.2.0 — 2026-04-27

### Web Fetch & Universal Scraping

A major expansion of Avalon's network capabilities, moving from hardcoded GitHub-only support to configurable universal web access with layered security.

#### New Backend Infrastructure

- **`WebFetchConfig`** (`src/main.rs`)
  - Persistent config stored in `.avalon_state.json` under the `web_fetch` key.
  - Fields: `max_depth`, `confirm_domains`, `allowed_domains`, `blocked_domains`, `timeout_secs`, `max_size_mb`, `respect_robots_txt`, `rate_limit_ms`.
  - Defaults to safe values: depth=1, confirm_domains=true, timeout=10s, max_size=5MB.

- **New API Endpoints**
  - `GET /api/web/config` — returns current `WebFetchConfig` as JSON.
  - `POST /api/web/config` — accepts a `WebFetchConfig` body and persists it.

- **`ToolContext` Expansion** (`src/tools/mod.rs`)
  - Added `web_fetch: &WebFetchConfig` so all network tools read live config instead of hardcoded constants.

#### fetch_url Tool Overhaul (`src/tools/fetch_tool.rs`)

- **Removed hardcoded GitHub-only restriction.** Now supports any public `http`/`https` URL subject to domain lists and confirmation settings.
- **Image support.** If the response `Content-Type` starts with `image/`, the tool returns a base64-encoded payload instead of text.
- **PDF support.** If the response `Content-Type` is `application/pdf`, the tool parses the PDF with `lopdf`, extracts plain text from all pages, and returns it. No scripts, forms, launch actions, or embedded files survive — only extracted text.
- **HTML sanitization.** Text responses with `text/html` are run through `sanitize_html()`, which strips `<script>`, `<style>`, `<iframe>`, `<form>`, `<nav>`, `<footer>`, `<header>`, `<aside>`, `<menu>`, `<noscript>`, and all inline event handlers (`on*=`).
- **Dynamic limits.** Reads `timeout_secs`, `max_size_mb`, `allowed_domains`, `blocked_domains`, and `confirm_domains` from `WebFetchConfig`.
- **Enhanced security.** Added URL scheme blocking (`file://`, `javascript:`, `data:`, `ftp://`) in addition to existing SSRF private-IP protection.

#### New web_scrape Tool (`src/tools/web_scrape_tool.rs`)

- **Recursive BFS crawler.** Takes a start URL and `max_depth` (overridable via input, defaults to config). Follows links breadth-first up to the configured depth.
- **Same-domain restriction.** Only crawls pages within the same domain as the start URL to prevent accidental internet-wide crawling.
- **robots.txt respect.** Fetches and parses `robots.txt` before crawling a domain. Honors `Disallow` paths.
- **Rate limiting.** Enforces a per-domain cooldown (`rate_limit_ms`) between requests.
- **Content extraction.** For each page:
  - Extracts `<title>`.
  - Sanitizes HTML and converts it to clean plain text.
  - Extracts image `src` URLs (resolved to absolute).
  - Queues discovered links for the next depth level.
- **Returns structured JSON:** `{ pages: [{ url, title, text, images: [] }] }`.
- **Security:** Same layered protections as `fetch_url` — scheme blocking, SSRF private-IP blocking, DNS rebind checks, domain allow/block lists, content-type guards, size limits, timeouts.

#### Frontend Settings UI (`client/ui/app.js`)

- **New "Web Fetch" expandable section** in the Settings panel with:
  - Number input: Max depth (1–10)
  - Number input: Timeout (1–120 sec)
  - Number input: Max size (1–100 MB)
  - Checkbox: Confirm unknown domains
  - Checkbox: Respect robots.txt
  - Editable list: Allowed domains (with Add/Remove)
  - Editable list: Blocked domains (with Add/Remove)
- All changes save immediately via `POST /api/web/config`.

#### Dependencies (`Cargo.toml`)

- Added `base64 = "0.22"` for image base64 encoding.
- Added `lopdf = "0.34"` for PDF text extraction.

#### Direct Fetch API (`/api/fetch`)

- **New endpoint** `POST /api/fetch` bypasses the AI tool-calling gatekeeping while preserving all backend security.
- Runs the identical safe fetch pipeline (URL validation, SSRF blocking, content sanitization, size limits, domain checks, PDF text extraction, image base64, HTML sanitization).
- Returns sanitized content directly to the frontend for user review before it is sent to the model.
- Useful when models refuse to call `fetch_url` due to baked-in safety training (e.g., PDF fetching on some local models).
- **Frontend UI:** New &#x2193; (download) button in the chat input bar opens a prompt for a URL, fetches directly, and displays the sanitized result in the chat history.

#### Documentation (`Docs/CAPABILITIES.md`)

- Updated tool descriptions for `fetch_url` and added `web_scrape`.
- Added the Web Fetch config reference table.
- Added the full security model table.
- Added `GET/POST /api/web/config` to the API endpoints list.
- Added `POST /api/fetch` to the API endpoints list with request/response examples.
- Added new source files to the Files & Config table.
- Added Settings panel section mentioning Web Fetch controls.

### System Prompt Update (`src/main.rs`)

- Updated the tools description injected into the LLM system prompt to describe the new `fetch_url` behavior (any URL, images, PDFs) and the new `web_scrape` recursive scraper.

### Compilation Status

- All modules compile cleanly under `cargo build --release`.
- One benign warning remains: `set_root` in `mindmap.rs` is declared but unused. It is safe to leave.

---

## 0.3.0 — 2026-04-28

### Video Analysis Tool

- New `analyze_video` tool (`src/tools/video_tool.rs`)
- Extracts metadata via `ffprobe`, keyframes as base64 via `ffmpeg`, and embedded subtitle tracks
- Configurable `interval_seconds` and `max_frames`
- Requires `ffmpeg` installed on the host system

### Security Config Settings

- New `SecurityConfig` struct in `src/main.rs` stored in `.avalon_state.json` under the `security` key
- Settings panel section with toggles:
  - `block_private_ips` — SSRF private IP blocking
  - `enforce_html_sanitize` — strip scripts/styles/iframes from fetched HTML
  - `require_write_permission` — block `write_file` unless approved
  - `require_delete_permission` — block `delete_file` unless approved
- Endpoints: `GET/POST /api/security/config`
- Enforced in `fetch_tool.rs`, `fs_tools.rs`, and the fetch pipeline

### Spell Check

- Right-click context menu on words in chat messages
- `POST /api/spellcheck` endpoint returns suggestions
- Canvas-based word detection under the cursor
- Click a suggestion to replace the word inline

### UI Improvements

- **Chat reset button** (X icon in header) clears chat history without restarting
- **Mindmap reset button** fixed — simulation is now non-blocking, chunked via `requestAnimationFrame`
- **Debug save** exports comprehensive Markdown with chat history + debug log to `logs/debug/`
- **Direct Fetch button** (down arrow) in the input bar for direct URL fetching

### Launcher Scripts

- `StartAvalonDesktop.vbs` — double-click Electron launcher, auto-detects release/debug build
- `StartAvalon.vbs` — browser launcher with backend wait logic
- `CreateShortcuts.ps1` — creates Desktop and Start Menu shortcuts for both variants
- No PowerShell terminal required for daily use
- Electron app auto-starts backend on launch and kills it on quit

### Backend Improvements

- **Multi-round tool execution** — up to 3 inference rounds per user message, allowing the AI to chain reads, analysis, and synthesis
- **Hardened system prompt** with 5 explicit rules: ALWAYS USE TOOLS, GO DEEP, NEVER DESCRIBE CONTEXT DATA GENERICALLY, CREATE STRUCTURED REPORTS, NEVER REFUSE TO USE TOOLS
- **Default model** changed from `llama3` to `qwen2.5-coder:32b`
- **Mindmap capping** — limited to 80 nodes and depth 1 to prevent local model timeouts on large codebases
- **Audit log** `save_to_file()` writes to `logs/debug/` instead of `logs/audit/`
- **Chain-of-custody export** fixed to actually write the Markdown file to disk

### Root Cleanup

- Removed 15+ clutter files from `D:\Avalon\`
- Consolidated duplicate documentation

---

## 0.4.0 — 2026-04-28

### MindVault — Persistent Document Ingestion & Search

- **SQLite + FTS5** full-text search backend (`src/db.rs`)
  - `vault_documents` table with SHA-256 content hashing for deduplication
  - `vault_fts` virtual table for ranked full-text search across titles and content
  - Automatic FTS5 index maintenance via SQLite triggers
- **VaultService** (`src/vault.rs`)
  - `ingest_file(path)` — reads text, PDF, HTML, markdown, and code files; extracts content; sanitizes; stores
  - `ingest_text(source, title, content, type)` — direct ingestion for fetched/scraped content
  - `search(query, limit)` — FTS5 ranked search returning `VaultDoc` records
  - `get(id)` / `delete(id)` — retrieve or remove documents
  - Content sanitization: strips null bytes, control characters, normalizes whitespace
  - PDF text extraction via `lopdf` (no scripts or embedded objects executed)
  - HTML sanitization: strips `<script>`, `<style>`, `<iframe>`, `<form>`, event handlers
- **Auto-ingest hooks**
  - `write_file` tool automatically ingests written files into MindVault
  - `fetch_url` tool automatically ingests fetched PDFs and text content
  - `web_scrape` tool automatically ingests scraped page text
- **New tools**
  - `vault_search` — query the MindVault by FTS5
  - `vault_read` — retrieve a single document by ID
- **New endpoints**
  - `GET /api/vault/search?q=&limit=` — search documents
  - `GET /api/vault/document/{id}` — retrieve document
  - `DELETE /api/vault/document/{id}` — delete document

### VisionVault — Image Library with Searchable Metadata

- **SQLite + FTS5** for image descriptions and tags (`src/db.rs`)
  - `vision_images` table with format, dimensions, hash, and confirmation status
  - `vision_fts` virtual table for searching descriptions and tags
- **VisionService** (`src/vision.rs`)
  - `ingest_image(path, suggested_description)` — detects format and dimensions from magic bytes, stores metadata
  - `confirm_description(id, description, tags)` — user-reviewed description confirmation
  - `search(query, limit)` — FTS5 search across descriptions and tags
  - `get(id)` / `delete(id)` — retrieve or remove image records
  - Format detection: PNG, JPEG, GIF, WebP, BMP, SVG
  - Dimension extraction from image headers (no external libraries needed)
- **Auto-ingest hook**
  - `read_image` endpoint (`/api/fs/image`) automatically ingests successfully read images into VisionVault
- **New tools**
  - `vision_search` — query the VisionVault by description/tags
  - `vision_read` — retrieve a single image record by ID
- **New endpoints**
  - `GET /api/vision/search?q=&limit=` — search images
  - `GET /api/vision/image/{id}` — retrieve image metadata
  - `POST /api/vision/confirm/{id}` — confirm/update description and tags
  - `DELETE /api/vision/image/{id}` — delete image record

### Secure Agent System

- **AgentRegistry** (`src/agents.rs`)
  - Agents stored in SQLite `agents` table with whitelisted `allowed_tools`
  - Built-in agents protected from modification/deletion (`is_builtin` flag)
  - Forbidden tools enforced at creation time: `bash`, `shell`, `exec`, `eval`, `create_agent`, `delete_agent`, `update_agent`
  - CRUD operations: `list_agents`, `get_agent`, `create_agent`, `update_agent`, `delete_agent`
- **Dispatch & Board**
  - `agent_dispatches` table tracks task status (`pending`, `running`, `completed`, `failed`, `cancelled`)
  - `agent_board` table for inter-agent messaging per dispatch
  - `agent_memory` table for session summaries
- **New tools**
  - `dispatch_agent` — creates a dispatch record for an agent task
  - `board_post` — post a message to a dispatch board
  - `board_read` — read messages from a dispatch board
- **New endpoints**
  - `GET /api/agents` — list all agents
  - `POST /api/agents` — create agent
  - `GET /api/agents/{name}` — get agent details
  - `POST /api/agents/{name}` — update agent (non-built-in only)
  - `DELETE /api/agents/{name}` — delete agent (non-built-in only)
  - `POST /api/agents/dispatch` — dispatch an agent
  - `GET /api/agents/dispatch/{id}` — get dispatch status
  - `POST /api/agents/dispatch/{id}/cancel` — cancel dispatch
  - `GET /api/agents/dispatch/{id}/board` — read board posts
  - `POST /api/agents/dispatch/{id}/board` — post to board

### Agent Worker Extension Point

- **Stub trait** (`src/agent_workers.rs`)
  - `AgentWorker` trait for external processes that register as agents
  - `WorkerRegistry` for managing loaded workers
  - `HttpImageWorker` placeholder for future image generation integration
  - Workers communicate via HTTP or stdin/stdout, isolating heavy models from the core runtime

### Security Guarantees

After this release:
- **No shell execution** — `bash`, `shell`, `exec`, `eval` tools do not exist in the registry and cannot be added to agents
- **No agent self-modification** — agents cannot create, delete, or modify other agents or their own config via tools
- **All external data sanitized** — files, fetches, and scrapes pass through HTML/JS stripping, null-byte removal, and control-character filtering before vault storage
- **Agent whitelist enforcement** — agents can only use tools explicitly allowed at creation time
- **Built-in immutability** — built-in agents cannot be deleted or modified
- **Path traversal impossible** — `FileSystemService` limiter enforces bounds on all file operations

---

## Prior Versions

For earlier changes (mindmap graph builder, plugin architecture, Electron frontend, security manager, file system limiter, permission system), see the git history.
