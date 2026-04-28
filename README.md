# Avalon

**A secure, local-first AI coding assistant with plugin-based tools.**

Avalon bridges a local language model (via Ollama or any OpenAI-compatible API) with your local file system. It runs entirely on your machine — your code never leaves your computer unless you explicitly choose a cloud API.

**Rust 1.75+ | Electron 30+ | AGPL v3 | Active Development**

---

## What is Avalon?

Avalon is a desktop AI harness that gives you a chat interface to a language model with direct, secure access to your local files and the web. Unlike browser-based assistants, Avalon lives on your machine and can read, write, and reason about your actual codebase.

### Key Features

- **Chat Interface** — Real-time SSE streaming with reasoning extraction and tool call visualization
- **File System Tools** — AI can read, write, list, and delete files (gated by configurable allow/deny lists)
- **Mind Map** — Automatic codebase graph building that injects file relationships into AI context
- **Web Fetch** — Download content from any URL: text, images (base64), and PDFs (safe text extraction)
- **Web Scrape** — Recursive BFS crawler with robots.txt respect, rate limiting, and same-domain bounds
- **Remote Mind Map** — Download public GitHub repos as zip, build structural graphs, merge with local
- **Permission System** — User approval dialogs for write/delete operations with session-scoped grants
- **Audit Logging** — Cryptographic hash-chain logging with hot/warm/cold tier storage for legal compliance
- **Debug Logging** — Comprehensive event log with save-to-Markdown export
- **Plugin Architecture** — Toggle optional tools on/off; core tools always active

---

## Architecture

| Component | Technology | Responsibility |
|-----------|-----------|----------------|
| Backend | Rust / Actix-web / reqwest | Model inference, tool execution, file system gating, permissions, audit logging, mind map generation |
| Frontend | Electron / Vanilla JS | Chat UI, settings panel, permission dialogs, SSE streaming, mind map viewer |
| Model | Ollama (local) or OpenAI API | LLM inference |
| Storage | `.avalon_fs.json`, `.avalon_state.json`, `logs/` | Persistent config and debug logs |

Avalon communicates over a local HTTP API at `http://127.0.0.1:8080`. The backend binds to `127.0.0.1` only — it is not exposed to your network.

---

## Core Capabilities

### Plugin-Based Tools

| Tool | Description | Type |
|------|-------------|------|
| `read_file` | Reads file contents | Core |
| `write_file` | Writes or overwrites a file | Core |
| `list_dir` | Lists files and directories | Core |
| `delete_file` | Deletes a file or directory | Core |
| `get_fs_config` | Reads the file system limiter config | Core |
| `mindmap` | Scans codebase and builds a relationship graph | Optional |
| `fetch_url` | Downloads content from any URL (text, images, PDFs) | Optional |
| `remote_mindmap` | Downloads a GitHub repo zip, builds mind map, merges, cleans up | Optional |
| `web_scrape` | Recursively scrapes a website with configurable depth | Optional |

Core tools are always active. Optional tools can be toggled in Settings > Plugins.

### Mind Map (Automatic Context)

When you use exploratory language like *"research my codebase"* or *"how is this structured,"* Avalon automatically builds a graph of your files and their relationships and injects it into the AI's context before it answers.

### Image Support

Avalon can read images from your file system or the web. Images are displayed inline in chat and rendered as thumbnails in the mind map viewer. All images are scanned for hidden data and steganography; any anomalies are reported and stripped before display.

### Security Model

- **File System:** Allow/deny path lists, size limits, user approval for writes
- **Network:** SSRF blocking, private IP filtering, domain allow/block lists, content-type guards, HTML sanitization, robots.txt respect, rate limiting
- **Audit:** SHA-256 hash chains, Merkle roots, append-only logs, WORM warm/cold archives
- **No execution:** Downloaded content is parsed, never executed

See [Docs/SECURITY_PROTOCOL.md](Docs/SECURITY_PROTOCOL.md) for full details.

---

## Documentation

| Document | Purpose |
|----------|---------|
| [Docs/CAPABILITIES.md](Docs/CAPABILITIES.md) | Full feature reference, API endpoints, config formats |
| [Docs/ARCHITECTURE_SPEC.md](Docs/ARCHITECTURE_SPEC.md) | System architecture, data flow, plugin system |
| [Docs/SECURITY_PROTOCOL.md](Docs/SECURITY_PROTOCOL.md) | Threat model, security layers, audit logging |
| [Docs/CONTINGENCY.md](Docs/CONTINGENCY.md) | Current state, limitations, recovery procedures |
| [INSTALL.md](INSTALL.md) | Installation and first-time setup guide |
| [Docs/CHANGELOG.md](Docs/CHANGELOG.md) | Version history and feature additions |
| [Docs/Avalon-Tree.md](Docs/Avalon-Tree.md) | Clean source tree |

---

## Quick Start

```bash
git clone https://github.com/MattRidge/Avalon.git
cd Avalon
```

See [INSTALL.md](INSTALL.md) for detailed setup instructions.

---

## License

Dual-licensed under **AGPL v3** and a commercial license.

Contact `legal@imperatormorsus.com` for commercial licensing inquiries.

---

*Built with Rust, Electron, and a lot of coffee.*
