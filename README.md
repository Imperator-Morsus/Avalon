# Avalon

A context-aware AI coding assistant that bridges a local language model with your file system through a secure, plugin-based architecture.

![License](https://img.shields.io/badge/license-MIT-blue.svg)![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey.svg)![Rust](https://img.shields.io/badge/backend-Rust-orange.svg)![Electron](https://img.shields.io/badge/frontend-Electron-blue.svg)

## What is Avalon?

Avalon is a local-first AI harness that lets you chat with an LLM while giving it controlled, permission-gated access to your codebase. It runs entirely on your machine, connects to Ollama or any OpenAI-compatible API, and understands your project structure before it answers.

## Features

* **Local & Private** — Runs on `127.0.0.1:8080`. No data leaves your machine unless you choose a cloud API.
* **Plugin Architecture** — Tools are hot-swappable plugins. Activate or deactivate them per session.
* **File System Limiter** — Granular allow/deny path rules with a deny-by-default security model.
* **Permission Gating** — Write and delete operations require explicit user approval.
* **Mind Map** — Automatically scans your codebase and builds a graph of files and imports, injected into context when you ask exploratory questions.
* **Intent Detection** — Says "research my codebase" and Avalon builds the mind map before the AI responds.
* **Custom AI Name** — Rename your assistant (Merlin, Friday, JARVIS, etc.) in settings.
* **Real-time Debug Log** — Watch every tool call, permission request, and inference turn in the debug panel.
* **SSE Streaming** — Responses stream token-by-token with reasoning extraction.

## Architecture

| Layer | Technology | Responsibility |
| --- | --- | --- |
| Backend | Rust / Actix-web | Model inference, tool execution, file system gating, permissions, debug logging, mind map generation |
| Frontend | Electron / Vanilla JS | Chat UI, settings panel, permission dialogs, SSE streaming, mind map viewer |
| Model | Ollama (local) or OpenAI API | LLM inference |
| Storage | `.avalon_fs.json`, `.avalon_state.json`, `logs/` | Persistent config and debug logs |

## Quick Start

### Prerequisites

* [Rust](https://rustup.rs/) (latest stable)
* [Node.js](https://nodejs.org/) (for Electron frontend)
* [Ollama](https://ollama.com/) (optional, for local models)

### Installation

    git clone https://github.com/YOUR_USERNAME/Avalon.git
    cd Avalon
    
    # Build the Rust backend
    cargo build --release
    
    # Install frontend dependencies
    cd client
    npm install
    cd ..

### Running

    # Option 1: Python launcher (recommended)
    python launch.py
    
    # Option 2: Electron directly
    cd client
    npm start

The app window opens at `1400x900`. The backend starts automatically and shuts down when you close the app.

### Environment Variables

Create a `.env` file in the project root:

    # Ollama (default)
    AVALON_MODEL_API_BASE=http://localhost:11434
    AVALON_MODEL_NAME=llama3
    
    # Or OpenAI / compatible API
    # AVALON_MODEL_API_BASE=https://api.openai.com/v1
    # AVALON_MODEL_API_KEY=sk-...

## Default Tools

| Tool | Description | Permission Required |
| --- | --- | --- |
| `read_file` | Reads file contents | No  |
| `write_file` | Writes or overwrites a file | Yes |
| `list_dir` | Lists files and directories | No  |
| `delete_file` | Deletes a file or directory | Yes |
| `get_fs_config` | Reads the file system limiter config | No  |
| `build_mindmap` | Scans allowed paths and builds a codebase graph | No  |

## API Endpoints

| Method | Path | Description |
| --- | --- | --- |
| GET | `/api/models` | List available models |
| GET/POST | `/api/model` | Get/set current model |
| GET | `/api/preload` | Preload a model in Ollama |
| GET | `/api/chat` | SSE chat stream |
| GET | `/api/tools` | List all registered tools |
| POST | `/api/plugins` | Set active tools |
| GET/POST | `/api/ai_name` | Get/set AI assistant name |
| GET | `/api/mindmap` | Get the codebase graph |
| GET/POST | `/api/fs/config` | Get/set file system limiter config |
| GET | `/api/permissions` | List active permissions |
| DELETE | `/api/permissions/{tool}` | Revoke a permission |

## Adding a Custom Tool

1. Create `src/tools/my_tool.rs`:

    use serde_json;
    use crate::tools::{Tool, ToolContext};
    
    pub struct MyTool;
    
    #[async_trait::async_trait]
    impl Tool for MyTool {
        fn name(&self) -> &str { "my_tool" }
        fn description(&self) -> &str { "Does something useful." }
        async fn execute(&self, input: serde_json::Value, ctx: &ToolContext<'_>) -> Result<serde_json::Value, String> {
            Ok(serde_json::json!({"result": "success"}))
        }
    }

2. Register it in `src/main.rs`:

    registry.register(Box::new(tools::my_tool::MyTool));

3. Rebuild. The tool appears automatically in the system prompt, the tool list API, and the plugin settings.

## Documentation

* [Capabilities & Functions](Docs/CAPABILITIES.md) — Full feature reference
* [Architecture Spec](Docs/ARCHITECTURE_SPEC.md) — Design decisions and data flow
* [Security Protocol](Docs/SECURITY_PROTOCOL.md) — Threat model and mitigation
* [Contingency](Docs/CONTINGENCY.md) — Backup and recovery procedures

## Security

* **CORS** restricted to `127.0.0.1`, `localhost`, `file://`, and `null` origins only.
* **File System Limiter** uses deny-by-default with explicit allow lists.
* **Permission Gating** requires user approval for all write/delete operations per session.
* **No secrets** are committed; API keys live in `.env` only.

## License

MIT License — see [LICENSE](LICENSE) for details.
