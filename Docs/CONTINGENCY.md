# Avalon Contingency Document

## Project Overview

**Goal:** A secure, local-first AI coding harness that bridges local LLMs (via Ollama) or cloud APIs with the local file system through a plugin-based tool architecture.

**Architecture:** Rust Backend (Actix-web) ↔ JSON/SSE ↔ Electron Frontend (Vanilla JS).

**Key Components:**
1. **Backend:** Rust using `actix-web`. Manages inference, tool execution, file system gating, and security.
2. **Frontend:** Electron with Vanilla JS. Chat UI, settings, permission dialogs, SSE streaming.
3. **Contract:** `InferenceRequest` / `InferenceResponse` (defined in `src/main.rs`).
4. **Abstraction:** The `ModelInferenceService` trait decouples the backend from any specific LLM provider.
5. **Plugin System:** The `Tool` trait and `ToolRegistry` allow adding new capabilities without changing core code.

---

## Current Project State

**Status:** Core system is **feature-complete** and stable.

### Completed Features

- **Model Inference Service** — OpenAI-compatible API and Ollama support with SSE streaming
- **File System Limiter** — Configurable allow/deny paths with size limits
- **Permission System** — Session-scoped user approvals for write/delete operations
- **Plugin Architecture** — Core/optional tool separation with dynamic activation
- **Mind Map** — Automatic codebase graph building with import parsing (Rust, JS/TS, Python)
- **Web Fetch** — Universal URL fetching with layered security (SSRF, private-IP, domain gates)
- **Web Scrape** — Recursive BFS crawler with robots.txt and rate limiting
- **PDF Support** — Safe text extraction without executing embedded scripts
- **Debug Logging** — Comprehensive event log with save-to-file capability
- **Settings UI** — Model selection, AI naming, file system rules, web fetch config, plugin activation
- **Electron Lifecycle** — Auto-starts backend, auto-kills on quit

### Active Development Areas

- Polish and edge-case hardening
- Documentation upkeep
- Testing and validation of new web tools

---

## Scope Constraints

- **No cloud dependency required.** Everything runs locally except the optional LLM API call.
- **No database.** State is stored in flat JSON files.
- **No containerization.** Direct binary execution.
- **Frontend is Vanilla JS, not React.** This keeps the bundle small and avoids framework churn.

---

## Known Limitations

- Mind map import resolution is regex-based and may miss dynamic imports or complex module paths.
- `web_scrape` is limited to same-domain crawling to prevent accidental wide-area scraping.
- PDF extraction relies on `lopdf`; some complex PDF layouts may not extract cleanly.
- No persistent conversation memory across restarts (history is per-session only).

---

## Recovery Procedures

### Backend fails to start
1. Check that port 8080 is not in use.
2. Run `cargo run --release` manually to see stderr.
3. Verify `.avalon_state.json` is valid JSON (delete it to reset if corrupted).

### Frontend shows "Backend unreachable"
1. Confirm the backend process is running.
2. Check the backend log for startup errors.
3. Verify the `API_BASE` in `client/ui/app.js` matches the backend bind address.

### Model returns garbage or tool calls fail
1. Check that the model is compatible with the system prompt format.
2. Smaller models may struggle with complex tool instructions.
3. Preload the model to ensure it is warm in Ollama memory.

### Settings not persisting
1. Check that `.avalon_state.json` and `.avalon_fs.json` are writable.
2. Verify the backend has permission to write to the project root directory.

---

## Contact

For issues or contributions, refer to the repository or contact `legal@imperatormorsus.com`.
