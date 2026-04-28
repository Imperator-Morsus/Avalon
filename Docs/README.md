# Avalon: Context-Aware AI Coding Harness

Avalon is a **local-first** AI coding assistant. Everything runs on your machine. Your code never leaves your computer unless you explicitly choose a cloud API.

## Overview

Avalon bridges a local language model (via Ollama or any OpenAI-compatible API) with your local file system through a secure, plugin-based tool architecture. It consists of a Rust backend (Actix-web) serving an Electron frontend, communicating over a local HTTP API at `127.0.0.1:8080`.

| Component | Technology | Responsibility |
|-----------|-----------|----------------|
| Backend | Rust / Actix-web / reqwest | Model inference, tool execution, file system gating, permissions, debug logging, mind map generation |
| Frontend | Electron / Vanilla JS | Chat UI, settings panel, permission dialogs, SSE streaming, mind map viewer |
| Model | Ollama (local) or OpenAI API | LLM inference |
| Storage | `.avalon_fs.json`, `.avalon_state.json`, `logs/` | Persistent config and debug logs |

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

### 1. Install Rust

Download and run the installer from [rustup.rs](https://rustup.rs/).

On Windows, download and run `rustup-init.exe` from the same site.

Verify:

```bash
cargo --version
```

### 2. Install Node.js

Download the LTS installer from [nodejs.org](https://nodejs.org/).

Verify:

```bash
node --version
npm --version
```

### 3. Install Ollama (for local models)

Download from [ollama.com](https://ollama.com/) and follow the install instructions.

Pull a model:

```bash
ollama pull llama3
```

---

## Installation

### Step 1: Clone the Repository

```bash
git clone https://github.com/MattRidge/Avalon.git
cd Avalon
```

### Step 2: Configure Environment (Optional)

Create a `.env` file in the project root:

```env
# For local models (default)
AVALON_MODEL_API_BASE=http://localhost:11434/v1
AVALON_MODEL_NAME=llama3

# For cloud APIs
# AVALON_MODEL_API_BASE=https://api.openai.com/v1
# AVALON_MODEL_API_KEY=sk-your-key-here
```

If you skip this step, Avalon defaults to local Ollama settings.

### Step 3: Install Frontend Dependencies

```bash
cd client
npm install
cd ..
```

### Step 4: Build the Backend

```bash
cargo build --release
```

The first build takes a few minutes.

---

## Running Avalon

### Option A: Python Launcher (Recommended for Development)

```bash
python launch.py local
```

This automatically:
- Detects or starts Ollama
- Builds the backend (if needed)
- Starts the backend on `http://127.0.0.1:8080`
- Launches the Electron GUI
- Shuts everything down cleanly when you close the window

**Other modes:**

```bash
python launch.py cloud    # Uses cloud API (requires API key)
python launch.py dummy     # No real model (for UI testing)
```

### Option B: Manual Start

**Terminal 1 — Backend:**

```bash
cargo run --release
```

**Terminal 2 — Frontend:**

```bash
cd client
npm start
```

### Option C: Electron Only

If the backend is already running:

```bash
cd client
npm start
```

---

## First-Time Setup

When Avalon opens for the first time:

1. **Select a model** from the dropdown in the top-right header.
2. **Preload the model** (optional but recommended) to keep it warm in memory.
3. **Open Settings** (gear icon) to configure:
   - **AI Assistant Name** — what the AI calls itself
   - **File System Limiter** — which paths Avalon can read/write
   - **Web Fetch** — domain rules, depth limits, timeouts
   - **Plugins** — activate or deactivate tools

### File System Limiter Setup

By default, Avalon denies all file access. You must explicitly allow paths:

1. Open Settings > File System Limiter
2. Change **Default Policy** to `deny` (recommended)
3. Add paths to **Allowed Paths**:
   ```
   D:/Projects
   D:/Avalon/src
   ```
4. Add paths to **Denied Paths** if needed:
   ```
   C:/
   D:/Secrets
   ```
5. Set **Max File Size** (default: 10 MB)

Changes save immediately to `.avalon_fs.json`.

### Web Fetch Setup

By default, Avalon only allows GitHub domains. To enable any website:

1. Open Settings > Web Fetch
2. Uncheck **Confirm unknown domains** to allow any domain not explicitly blocked
3. Or add specific domains to **Allowed domains**
4. Adjust **Max depth**, **Timeout**, and **Max size** as needed

Changes save immediately to `.avalon_state.json`.

---

## Core Capabilities

### Plugin-Based Tool System

Tools are plugins that the AI can invoke. Core tools (file operations) are always active. Optional tools can be toggled:

| Tool | Description |
|------|-------------|
| `read_file` | Reads file contents |
| `write_file` | Writes or overwrites a file |
| `list_dir` | Lists files and directories |
| `delete_file` | Deletes a file or directory |
| `get_fs_config` | Reads the file system limiter config |
| `mindmap` | Scans codebase and builds a relationship graph |
| `fetch_url` | Downloads content from any URL (text, images, PDFs) |
| `remote_mindmap` | Downloads a GitHub repo zip, builds mind map, merges, cleans up |
| `web_scrape` | Recursively scrapes a website with configurable depth |

### Mind Map (Automatic Context)

When you use exploratory language like "research my codebase" or "how is this structured," Avalon automatically builds a graph of your files and their relationships, injecting it into the AI's context before it answers.

### Security Model

Layered protections across file system and network:
- **File System:** Allow/deny path lists, size limits, user approval for writes
- **Network:** SSRF blocking, private IP filtering, domain allow/block lists, content-type guards, HTML sanitization, robots.txt respect, rate limiting
- **No execution:** Downloaded content is parsed, never executed

See `SECURITY_PROTOCOL.md` for full details.

---

## Documentation

| Document | Purpose |
|----------|---------|
| `Docs/CAPABILITIES.md` | Full feature reference, API endpoints, config formats |
| `Docs/ARCHITECTURE_SPEC.md` | System architecture, data flow, plugin system |
| `Docs/SECURITY_PROTOCOL.md` | Threat model, security layers, audit logging |
| `Docs/CONTINGENCY.md` | Current state, limitations, recovery procedures |
| `Docs/INSTALL.md` | Detailed installation and troubleshooting guide |
| `Docs/CHANGELOG.md` | Version history and feature additions |

---

## Updating Avalon

```bash
cd Avalon
git pull origin main
cargo build --release
```

If frontend dependencies changed:

```bash
cd client
npm install
cd ..
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

Make sure you are inside the `client/` directory when running `npm install`.

### GUI opens but backend is unreachable

The backend may have failed to start. Run it manually to see errors:

```bash
cargo run --release
```

### Permission denied on file writes

Avalon requires user approval for `write_file` and `delete_file` operations. A dialog appears in the chat area when the AI attempts these. Click **Approve** to grant the tool access for the session.

---

## Uninstalling

Avalon does not install anything system-wide. To remove it:

```bash
rm -rf Avalon/
```

Optional — remove local data:

```bash
rm ~/.avalon_state.json
rm ~/.avalon_fs.json
```

(Exact paths depend on your OS and where you placed the files.)

---

## License

Dual-licensed under AGPL v3 and a commercial license.
Contact `legal@imperatormorsus.com` for commercial licensing inquiries.

---

*Built with Rust, Electron, and a lot of coffee.*
