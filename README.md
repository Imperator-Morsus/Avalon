# Avalon

**A secure, local-first AI coding assistant with plugin-based tools.**

Avalon bridges a local language model (via Ollama or any OpenAI-compatible API) with your local file system. It runs entirely on your machine -- your code never leaves your computer unless you explicitly choose a cloud API.

**Rust 1.75+ | Electron 30+ | AGPL v3 | Active Development**

---

## What is Avalon?

Avalon is a desktop AI harness that gives you a chat interface to a language model with direct, secure access to your local files and the web. Unlike browser-based assistants, Avalon lives on your machine and can read, write, and reason about your actual codebase.

### Key Features

- **Chat Interface** -- Real-time SSE streaming with reasoning extraction and tool call visualization
- **File System Tools** -- AI can read, write, list, and delete files (gated by configurable allow/deny lists)
- **Mind Map** -- Automatic codebase graph building that injects file relationships into AI context
- **Web Fetch** -- Download content from any URL: text, images (base64), and PDFs (safe text extraction)
- **Web Scrape** -- Recursive BFS crawler with robots.txt respect, rate limiting, and same-domain bounds
- **Video Analysis** -- Extract metadata, keyframes, and subtitles from local video files (requires ffmpeg)
- **Remote Mind Map** -- Download public GitHub repos as zip, build structural graphs, merge with local
- **MindVault** -- Persistent document ingestion with SQLite + FTS5 full-text search; auto-ingests files, PDFs, and web scrapes
- **VisionVault** -- Image library with format detection, dimension extraction, and searchable descriptions/tags; auto-ingests on image read
- **Secure Agent System** -- Whitelist-based agents with forbidden tool enforcement, built-in protection, dispatch board, and session memory
- **Permission System** -- User approval dialogs for write/delete operations with session-scoped grants
- **Audit Logging** -- Cryptographic hash-chain logging with hot/warm/cold tier storage for legal compliance
- **Debug Logging** -- Comprehensive event log with save-to-Markdown export
- **Plugin Architecture** -- Toggle optional tools on/off; core tools always active

---

## Architecture

| Component | Technology                                                     | Responsibility                                                                                                                       |
| --------- | -------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| Backend   | Rust / Actix-web / reqwest                                     | Model inference, tool execution, file system gating, permissions, audit logging, mind map generation, vault services, agent registry |
| Frontend  | Electron / Vanilla JS                                          | Chat UI, settings panel, permission dialogs, SSE streaming, mind map viewer, vault search, agent management                        |
| Model     | Ollama (local) or OpenAI API                                   | LLM inference                                                                                                                        |
| Storage   | `.avalon_fs.json`, `.avalon_state.json`, `logs/`, `.avalon.db` | Persistent config, debug logs, SQLite vault (documents, images, agents)                                                              |

Avalon communicates over a local HTTP API at `http://127.0.0.1:8080`. The backend binds to `127.0.0.1` only -- it is not exposed to your network.

---

## Core Capabilities

### Plugin-Based Tools

| Tool             | Description                                                     | Type     |
| ---------------- | --------------------------------------------------------------- | -------- |
| `read_file`      | Reads file contents                                             | Core     |
| `write_file`     | Writes or overwrites a file                                     | Core     |
| `list_dir`       | Lists files and directories                                     | Core     |
| `delete_file`    | Deletes a file or directory                                     | Core     |
| `get_fs_config`  | Reads the file system limiter config                            | Core     |
| `mindmap`        | Scans codebase and builds a relationship graph                  | Optional |
| `fetch_url`      | Downloads content from any URL (text, images, PDFs)             | Optional |
| `remote_mindmap` | Downloads a GitHub repo zip, builds mind map, merges, cleans up | Optional |
| `web_scrape`     | Recursively scrapes a website with configurable depth           | Optional |
| `analyze_video`  | Extracts metadata, keyframes, and subtitles from local video    | Optional |
| `vault_search`   | Full-text search across ingested documents and PDFs             | Optional |
| `vault_read`     | Retrieves a document from MindVault by ID                       | Optional |
| `vision_search`  | Search images by description or tags in VisionVault             | Optional |
| `vision_read`    | Retrieves image metadata from VisionVault by ID                 | Optional |
| `dispatch_agent` | Dispatches a secure agent to perform a task                     | Optional |
| `board_post`     | Posts a message to an agent dispatch board                      | Optional |
| `board_read`     | Reads messages from an agent dispatch board                     | Optional |

Core tools are always active. Optional tools can be toggled in Settings > Plugins.

### Mind Map (Automatic Context)

When you use exploratory language like *"research my codebase"* or *"how is this structured,"* Avalon automatically builds a graph of your files and their relationships and injects it into the AI's context before it answers.

### MindVault (Persistent Document Storage)

Whenever Avalon writes a file, fetches a URL, or scrapes a web page, the content is automatically ingested into a local SQLite database with FTS5 full-text search.

- Search across all ingested documents via `vault_search` or the Vault UI
- PDFs are safely parsed to plain text (no scripts executed)
- HTML and scripts are stripped before storage
- SHA-256 deduplication prevents duplicate entries

### VisionVault (Image Library)

Images read by Avalon are automatically ingested with format detection and dimension extraction.

- Search images by description or tags via `vision_search` or the Vault UI
- AI-suggested descriptions can be confirmed or edited
- Supports PNG, JPEG, GIF, WebP, BMP, SVG

### Image Support

Avalon can read images from your file system or the web. Images are displayed inline in chat and rendered as thumbnails in the mind map viewer. All images are scanned for hidden data and steganography; any anomalies are reported and stripped before display.

### Secure Agent System

Agents are stored in SQLite with strictly whitelisted tools. They run in the same async event loop as chat -- no background threads, no auto-approved permissions, and no shell execution.

- **Forbidden tools:** `bash`, `shell`, `exec`, `eval`, `create_agent`, `delete_agent`, `update_agent` can never be added to an agent
- **Built-in protection:** Built-in agents cannot be modified or deleted
- **Permission pipeline:** Agent tool calls require the same user approval as your own tool calls
- **Dispatch board:** Agents post results to a per-task message board
- **Session memory:** Agent summaries are stored for continuity

### Video Analysis

Analyze local video files with the `analyze_video` tool. Requires ffmpeg installed on the host system.

- Extracts metadata (codec, duration, resolution) via `ffprobe`
- Extracts keyframes as base64 images via `ffmpeg`
- Reads embedded subtitle tracks

### Security Model

- **File System:** Allow/deny path lists, size limits, user approval for writes
- **Network:** SSRF blocking, private IP filtering, domain allow/block lists, content-type guards, HTML sanitization, robots.txt respect, rate limiting
- **Security Settings:** Configurable toggles for private IP blocking, HTML sanitization, and write/delete permission requirements
- **Audit:** SHA-256 hash chains, Merkle roots, append-only logs, WORM warm/cold archives
- **No execution:** Downloaded content is parsed, never executed

See [Docs/SECURITY_PROTOCOL.md](Docs/SECURITY_PROTOCOL.md) for full details.

---

## Documentation

| Document                                               | Purpose                                                        |
| ------------------------------------------------------ | -------------------------------------------------------------- |
| [Docs/CAPABILITIES.md](Docs/CAPABILITIES.md)           | Full feature reference, API endpoints, config formats          |
| [Docs/ARCHITECTURE_SPEC.md](Docs/ARCHITECTURE_SPEC.md) | System architecture, data flow, plugin system                  |
| [Docs/SECURITY_PROTOCOL.md](Docs/SECURITY_PROTOCOL.md) | Threat model, security layers, audit logging                   |
| [Docs/CONTINGENCY.md](Docs/CONTINGENCY.md)             | Current state, limitations, recovery procedures                |
| [INSTALL.md](INSTALL.md)                               | Installation and first-time setup guide                        |
| [Docs/CHANGELOG.md](Docs/CHANGELOG.md)                 | Version history and feature additions                          |
| [Docs/Avalon-Tree.md](Docs/Avalon-Tree.md)             | Clean source tree                                              |
| [Docs/AGENTS.md](Docs/AGENTS.md)                       | Agent system reference, creation, dispatch, and security model |

---

## Quick Start

```bash
git clone https://github.com/Imperator-Morsus/Avalon.git
cd Avalon
```

See [INSTALL.md](INSTALL.md) for detailed setup instructions.

---

## License

Dual-licensed under **AGPL v3** and a commercial license.

Contact `Cyberpawz@gmail.com` for commercial licensing inquiries.

---

*Built with Rust, Electron, and a lot of coffee.*
