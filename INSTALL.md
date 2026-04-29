# Installation Guide

## Quick Overview

Avalon is a **local-first** AI coding assistant. Everything runs on your machine. Your code never leaves your computer unless you explicitly choose a cloud API.

| Component | Required | Purpose |
|-----------|----------|---------|
| Rust toolchain | Yes | Compiles the backend server |
| Node.js + npm | Yes | Runs the Electron frontend |
| Ollama (or other) | Optional* | Local LLM inference |
| ffmpeg | Optional** | Video analysis (`analyze_video` tool) |
| Python 3 | No | Optional launcher (`launch.py`) |

*Only required if you use local models. Cloud APIs (OpenAI, etc.) work without it.

**Only required if you want to use the video analysis tool. Avalon will report that ffmpeg is missing if you try to use it without installing.

---

## Prerequisites

### 1. Rust

Download and run the installer from [rustup.rs](https://rustup.rs/).

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

On Windows, download and run `rustup-init.exe` from the same site.

Verify installation:

```bash
cargo --version
```

Expected output: `cargo 1.xx.x` (Avalon requires Rust 1.75+.)

### 2. Node.js + npm

Download the LTS installer from [nodejs.org](https://nodejs.org/).

Verify installation:

```bash
node --version
npm --version
```

Expected output: `v22.x.x` and `10.x.x` or newer.

### 3. Ollama (for local models)

Download from [ollama.com](https://ollama.com/) and follow the install instructions for your OS.

Pull a model (example):

```bash
ollama pull qwen2.5-coder:32b
```

Verify Ollama is running:

```bash
ollama list
```

### 4. ffmpeg (for video analysis)

Only needed if you plan to use the `analyze_video` tool.

**Windows:**
```powershell
winget install Gyan.FFmpeg
```

**macOS:**
```bash
brew install ffmpeg
```

**Linux (Ubuntu/Debian):**
```bash
sudo apt update && sudo apt install ffmpeg
```

Verify:
```bash
ffmpeg -version
ffprobe -version
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
AVALON_MODEL_NAME=qwen2.5-coder:32b

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

This downloads Electron and all frontend packages.

### Step 4: Build the Backend

```bash
cargo build --release
```

This compiles the Rust server. The first build takes a few minutes.

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
python launch.py dummy    # No real model (for UI testing)
```

### Option B: Manual Start

**Terminal 1 -- Backend:**

```bash
cargo run --release
```

**Terminal 2 -- Frontend:**

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

### Option D: Desktop Shortcuts (No Terminal Required)

After building, run the PowerShell shortcut creator once:

```powershell
.\CreateShortcuts.ps1
```

This creates two shortcuts on your Desktop and Start Menu:
- **Avalon Desktop** -- launches the Electron app (backend starts automatically)
- **Avalon Browser** -- starts the backend and opens Avalon in your default browser

Double-click either shortcut. No PowerShell window needed for daily use.

**Note:** The Electron app automatically starts the backend on launch and kills it on quit.

---

## First-Time Setup

When Avalon opens for the first time:

1. **Select a model** from the dropdown in the top-right header.
2. **Preload the model** (optional but recommended) to keep it warm in memory.
3. **Open Settings** (gear icon) to configure:
   - **AI Assistant Name** -- what the AI calls itself
   - **File System Limiter** -- which paths Avalon can read/write
   - **Security** -- toggle private IP blocking, HTML sanitization, write/delete permission requirements
   - **Plugins** -- activate or deactivate tools
   - **Audit Log** -- enable or disable warm/cold tier archiving
   - **Web Fetch** -- allowed domains, depth limits, robots.txt respect
   - **MindVault** -- max vault size, auto-ingest toggle
   - **Agents** -- create and manage secure agents

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

### MindVault

MindVault automatically ingests text files, PDFs, and scraped web pages into a local SQLite database with full-text search.

- **Auto-ingest** happens automatically when you use `write_file`, `fetch_url`, or `web_scrape`
- **Search** via the Vault button (vault icon in header) or ask the AI to use `vault_search`
- **Settings** > **MindVault** lets you toggle auto-ingest

### VisionVault

VisionVault stores image metadata and descriptions for searchable image retrieval.

- **Auto-ingest** happens automatically when Avalon reads an image via `read_file` or `/api/fs/image`
- **Confirm descriptions** via the Vault button > Images tab
- **Search** by description or tags via the AI tool `vision_search`

### Agent Setup

Agents are whitelisted AI workers that run inside Avalon's async loop.

1. Open Settings > Agents
2. Click **Create Agent**
3. Set name, role, system prompt, and allowed tools
4. Save

Agents cannot use shell execution tools and cannot modify themselves. All agent tool calls go through the same permission pipeline as your own.

---

## Updating Avalon

To update to the latest code:

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

### Video analysis says "ffmpeg not found"

Install ffmpeg and ensure `ffmpeg` and `ffprobe` are on your system PATH. See the Prerequisites section above.

### SQLite / vault errors on first start

Avalon creates `.avalon.db` in the working directory on first launch. If you see FTS5 errors, your SQLite build may not support FTS5. The bundled SQLite in `rusqlite` (used by Avalon) includes FTS5 by default. If you compiled SQLite separately, rebuild with FTS5 enabled.

---

## Uninstalling

Avalon does not install anything system-wide. To remove it:

```bash
rm -rf Avalon/
```

Optional -- remove local data:

```bash
rm ~/.avalon_state.json
rm ~/.avalon_fs.json
rm .avalon.db
rm -rf logs/
```

(Exact paths depend on your OS and where you placed the files.)
