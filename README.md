# Avalon

**A secure, local-first AI coding assistant with plugin-based tools.**

Avalon bridges a local language model (via Ollama or any OpenAI-compatible API) with your local file system. It runs entirely on your machine — your code never leaves your computer unless you explicitly choose a cloud API.

[![Rust](https://img.shields.io/badge/Rust-1.75%2B-orange?logo=rust)](https://www.rust-lang.org)
[![Electron](https://img.shields.io/badge/Electron-28%2B-47848F?logo=electron)](https://www.electronjs.org)
[![License](https://img.shields.io/badge/License-AGPL%20v3-blue)](LICENSE)
[![Status](https://img.shields.io/badge/Status-Active%20Development-brightgreen)]()

---

## Table of Contents

- [Features](#features)
- [Architecture](#architecture)
- [Quick Start](#quick-start)
- [Installation](#installation)
- [Running Avalon](#running-avalon)
- [First-Time Setup](#first-time-setup)
- [Core Capabilities](#core-capabilities)
- [Documentation](#documentation)
- [Updating](#updating)
- [Troubleshooting](#troubleshooting)
- [License](#license)

---

## Features

- **Chat Interface** — Real-time SSE streaming with reasoning extraction and tool call visualization
- **File System Tools** — AI can read, write, list, and delete files (gated by configurable allow/deny lists)
- **Mind Map** — Automatic codebase graph building that injects file relationships into AI context
- **Web Fetch** — Download content from any URL: text, images (base64), and PDFs (safe text extraction)
- **Web Scrape** — Recursive BFS crawler with robots.txt respect, rate limiting, and same-domain bounds
- **Remote Mind Map** — Download public GitHub repos as zip, build structural graphs, merge with local
- **Permission System** — User approval dialogs for write/delete operations with session-scoped grants
- **Debug Logging** — Comprehensive event log with save-to-Markdown export
- **Plugin Architecture** — Toggle optional tools on/off; core tools always active

---

## Architecture

| Component | Technology | Responsibility |
|-----------|-----------|----------------|
| Backend | Rust / Actix-web / reqwest | Model inference, tool execution, file system gating, permissions, debug logging, mind map generation |
| Frontend | Electron / Vanilla JS | Chat UI, settings panel, permission dialogs, SSE streaming, mind map viewer |
| Model | Ollama (local) or OpenAI API | LLM inference |
| Storage | `.avalon_fs.json`, `.avalon_state.json`, `logs/` | Persistent config and debug logs |

Avalon communicates over a local HTTP API at `http://127.0.0.1:8080`. The backend binds to `127.0.0.1` only — it is not exposed to your network.

---

## Quick Start

### Prerequisites

| Component | Required | Purpose |
|-----------|----------|---------|
| Rust toolchain | Yes | Compiles the backend server |
| Node.js + npm | Yes | Runs the Electron frontend |
| Ollama (or other) | Optional* | Local LLM inference |
| Python 3 | No | Optional launcher (`launch.py`) |

*Only required if you use local models. Cloud APIs (OpenAI, etc.) work without it.

Install Rust from [rustup.rs](https://rustup.rs/) and Node.js from [nodejs.org](https://nodejs.org/).

For local models, install Ollama from [ollama.com](https://ollama.com/):

```bash
ollama pull llama3
```

---

## Installation

```bash
git clone https://github.com/MattRidge/Avalon.git
cd Avalon
```

Optional — configure a cloud API or custom model in `.env`:

```env
# For local models (default)
AVALON_MODEL_API_BASE=http://localhost:11434/v1
AVALON_MODEL_NAME=llama3

# For cloud APIs
# AVALON_MODEL_API_BASE=https://api.openai.com/v1
# AVALON_MODEL_API_KEY=sk-your-key-here
```

Install frontend dependencies and build the backend:

```bash
cd client && npm install && cd ..
cargo build --release
```

---

## Running Avalon

### Option A: Python Launcher (Recommended)

```bash
python launch.py local
```

This automatically detects or starts Ollama, builds the backend if needed, launches the Electron GUI, and shuts everything down cleanly on exit.

```bash
python launch.py cloud   # Uses cloud API (requires API key)
python launch.py dummy   # No real model (for UI testing)
```

### Option B: Manual Start

Terminal 1 — Backend:

```bash
cargo run --release
```

Terminal 2 — Frontend:

```bash
cd client && npm start
```

---

## First-Time Setup

When Avalon opens:

1. **Select a model** from the dropdown in the header.
2. **Preload the model** (optional) to keep it warm in memory.
3. **Open Settings** (gear icon) to configure:

### File System Limiter

By default, Avalon denies all file access. Add paths to **Allowed Paths**:

```
D:/Projects
D:/Avalon/src
```

Set **Denied Paths** if needed:

```
C:/
D:/Secrets
```

Set **Max File Size** (default: 10 MB). Changes save to `.avalon_fs.json`.

### Web Fetch

By default, only GitHub domains are allowed. To enable any website:

1. Open Settings > Web Fetch
2. Uncheck **Confirm unknown domains** to allow any domain not explicitly blocked
3. Or add specific domains to **Allowed domains**
4. Adjust **Max depth**, **Timeout**, and **Max size** as needed

Changes save to `.avalon_state.json`.

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

### Security Model

- **File System:** Allow/deny path lists, size limits, user approval for writes
- **Network:** SSRF blocking, private IP filtering, domain allow/block lists, content-type guards, HTML sanitization, robots.txt respect, rate limiting
- **No execution:** Downloaded content is parsed, never executed

See `Docs/SECURITY_PROTOCOL.md` for full details.

---

## Documentation

| Document | Purpose |
|----------|---------|
| [Docs/CAPABILITIES.md](Docs/CAPABILITIES.md) | Full feature reference, API endpoints, config formats |
| [Docs/ARCHITECTURE_SPEC.md](Docs/ARCHITECTURE_SPEC.md) | System architecture, data flow, plugin system |
| [Docs/SECURITY_PROTOCOL.md](Docs/SECURITY_PROTOCOL.md) | Threat model, security layers, audit logging |
| [Docs/CONTINGENCY.md](Docs/CONTINGENCY.md) | Current state, limitations, recovery procedures |
| [Docs/INSTALL.md](Docs/INSTALL.md) | Detailed installation and troubleshooting guide |
| [Docs/CHANGELOG.md](Docs/CHANGELOG.md) | Version history and feature additions |
| [Docs/Avalon-Tree.md](Docs/Avalon-Tree.md) | Clean source tree |

---

## Updating

```bash
git pull origin main
cargo build --release
```

If frontend dependencies changed:

```bash
cd client && npm install && cd ..
```

---

## Troubleshooting

### "cargo not found"

Rust is not on your PATH. Restart your terminal or run:

```bash
source $HOME/.cargo/env
```

### "Backend exited early with code 1"

Check that Ollama is running:

```bash
ollama serve
```

Or verify your `AVALON_MODEL_API_BASE` is correct in `.env`.

### "npm install fails"

Make sure you are inside the `client/` directory.

### GUI opens but backend is unreachable

Run the backend manually to see errors:

```bash
cargo run --release
```

### Permission denied on file writes

Avalon requires user approval for `write_file` and `delete_file`. A dialog appears when the AI attempts these. Click **Approve** to grant access for the session.

---

## License

Dual-licensed under **AGPL v3** and a commercial license.

Contact `legal@imperatormorsus.com` for commercial licensing inquiries.

---

*Built with Rust, Electron, and a lot of coffee.*
