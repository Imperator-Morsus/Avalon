#!/usr/bin/env python3
"""
Avalon Launcher
===============
One-file startup script for the Avalon AI Harness.

What it does:
1. Detects or starts your local inference server (Ollama, LM Studio, etc.)
2. Builds and starts the Rust backend
3. Starts the Electron GUI (if available)

Usage:
    python launch.py [local|cloud|dummy]

Modes:
    local  - Use a local model server (default). Tries to auto-detect or start Ollama.
    cloud  - Use a cloud API (requires AVALON_MODEL_API_KEY).
    dummy  - Use the mock inference service (no real model).

Prerequisites:
    - Rust toolchain (cargo) installed and on PATH
    - For local mode: Ollama, LM Studio, or another OpenAI-compatible local server running,
      OR Ollama installed so this script can start it for you.
    - For GUI: Node.js and npm (only if client/package.json exists).
"""

import argparse
import os
import platform
import re
import shutil
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

SCRIPT_DIR = Path(__file__).resolve().parent
BACKEND_PORT = 8080
AVALON_EXE = SCRIPT_DIR / "target" / "release" / ("avalon_backend.exe" if platform.system() == "Windows" else "avalon_backend")

LOCAL_SERVERS = {
    "ollama": {"url": "http://localhost:11434", "start_cmd": ["ollama", "serve"]},
    "lm_studio": {"url": "http://localhost:1234", "start_cmd": None},
    "llamacpp": {"url": "http://localhost:8080", "start_cmd": None},
}

DEFAULT_LOCAL_BASE = "http://localhost:11434/v1"
DEFAULT_LOCAL_MODEL = "llama3"
DEFAULT_CLOUD_BASE = "https://api.openai.com/v1"
DEFAULT_CLOUD_MODEL = "gpt-4o-mini"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def log(msg, *, level="INFO"):
    prefix = {"INFO": "[Avalon]", "WARN": "[WARN]", "ERROR": "[ERROR]", "OK": "[OK]"}.get(level, level)
    print(f"{prefix} {msg}")


def run(cmd, *, cwd=None, capture=False, check=True, timeout=None, **kwargs):
    """Run a shell command, returning CompletedProcess."""
    shell = isinstance(cmd, str)
    return subprocess.run(
        cmd,
        cwd=cwd or SCRIPT_DIR,
        shell=shell,
        capture_output=capture,
        text=True,
        check=check,
        timeout=timeout,
        **kwargs,
    )


def is_port_open(host, port, timeout=1):
    import socket
    try:
        with socket.create_connection((host, port), timeout=timeout):
            return True
    except (socket.timeout, ConnectionRefusedError, OSError):
        return False


def wait_for_url(url, timeout=60, interval=0.5):
    """Poll until `url` responds with HTTP 200."""
    start = time.time()
    while time.time() - start < timeout:
        try:
            req = urllib.request.Request(url, method="GET")
            with urllib.request.urlopen(req, timeout=2) as resp:
                if resp.status == 200:
                    return True
        except Exception:
            pass
        time.sleep(interval)
    return False


def load_dotenv():
    """Load key=value pairs from .env if present."""
    env_path = SCRIPT_DIR / ".env"
    if not env_path.exists():
        return
    log("Loading environment from .env")
    with open(env_path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            if "=" in line:
                key, value = line.split("=", 1)
                os.environ.setdefault(key.strip(), value.strip().strip('"').strip("'"))


def check_executable(name):
    """Return True if `name` is found on PATH."""
    return shutil.which(name) is not None


def find_cargo():
    """Locate the cargo executable, including rustup default paths."""
    cargo = shutil.which("cargo")
    if cargo:
        return cargo

    # Rustup default install location
    if platform.system() == "Windows":
        cargo = Path(os.environ.get("USERPROFILE", "")) / ".cargo" / "bin" / "cargo.exe"
    else:
        cargo = Path(os.environ.get("HOME", "")) / ".cargo" / "bin" / "cargo"

    if cargo.exists():
        return str(cargo)

    return None


# ---------------------------------------------------------------------------
# Inference Server Management
# ---------------------------------------------------------------------------

def detect_local_server():
    """Check if any known local inference server is already running."""
    for name, cfg in LOCAL_SERVERS.items():
        host, port = cfg["url"].replace("http://", "").split(":")
        port = int(port)
        if is_port_open(host, port):
            log(f"Detected running {name} at {cfg['url']}")
            return cfg["url"]
    return None


def try_start_ollama():
    """Attempt to start `ollama serve` if Ollama is installed."""
    if not check_executable("ollama"):
        return False
    log("Starting Ollama server (`ollama serve`)...")
    try:
        # Spawn detached so it survives after this script exits if needed
        kwargs = {}
        if platform.system() == "Windows":
            kwargs["creationflags"] = subprocess.CREATE_NO_WINDOW
        else:
            kwargs["start_new_session"] = True
        subprocess.Popen(["ollama", "serve"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, **kwargs)
    except Exception as e:
        log(f"Failed to start Ollama: {e}", level="WARN")
        return False

    log("Waiting for Ollama to become ready...")
    if wait_for_url("http://localhost:11434", timeout=30):
        log("Ollama is ready.")
        return True
    log("Ollama did not respond in time.", level="WARN")
    return False


def ensure_inference_server(mode):
    """Make sure an inference endpoint exists before we start the backend."""
    if mode == "dummy":
        log("Dummy mode: no inference server needed.")
        return None, None

    if mode == "cloud":
        api_base = os.environ.get("AVALON_MODEL_API_BASE", DEFAULT_CLOUD_BASE)
        # Only pass model name if user explicitly set it; let backend persist otherwise
        model_name = os.environ.get("AVALON_MODEL_NAME", "")
        return api_base, model_name

    # local mode
    running_url = detect_local_server()
    if running_url:
        api_base = f"{running_url}/v1"
        model_name = os.environ.get("AVALON_MODEL_NAME", "")
        return api_base, model_name

    log("No local inference server detected.")
    if try_start_ollama():
        api_base = "http://localhost:11434/v1"
        model_name = os.environ.get("AVALON_MODEL_NAME", "")
        return api_base, model_name

    log("Could not auto-start a local inference server.", level="ERROR")
    log("Please start one manually (Ollama, LM Studio, llama.cpp, etc.)", level="ERROR")
    sys.exit(1)


# ---------------------------------------------------------------------------
# Backend
# ---------------------------------------------------------------------------

def kill_existing_backend():
    """Kill any running avalon_backend.exe processes on Windows."""
    if platform.system() != "Windows":
        return
    try:
        subprocess.run(["taskkill", "/F", "/IM", "avalon_backend.exe"],
                       capture_output=True, check=False)
    except Exception:
        pass


def build_backend():
    cargopath = find_cargo()
    if not cargopath:
        log("cargo not found on PATH.", level="ERROR")
        log("Please install Rust: https://rustup.rs/", level="ERROR")
        sys.exit(1)

    # Make sure no old backend is running (locks the exe on Windows)
    if is_port_open("127.0.0.1", BACKEND_PORT, timeout=0.5):
        log("Backend already running on port 8080 — stopping it first...")
        kill_existing_backend()
        time.sleep(1)

    log("Building Avalon backend (cargo build --release)...")
    try:
        run([cargopath, "build", "--release"], check=True)
    except subprocess.CalledProcessError:
        log("Backend build failed.", level="ERROR")
        sys.exit(1)
    log("Build complete.")


def start_backend(api_base, model_name, mode):
    log("Starting Avalon backend...")
    env = os.environ.copy()
    if mode != "dummy":
        env["AVALON_MODEL_API_BASE"] = api_base
        # Only override model name if user explicitly configured it.
        # Otherwise let the backend use its persisted state.
        if model_name:
            env["AVALON_MODEL_NAME"] = model_name

    kwargs = {}
    if platform.system() == "Windows":
        kwargs["creationflags"] = subprocess.CREATE_NO_WINDOW
    else:
        kwargs["start_new_session"] = True

    backend_proc = subprocess.Popen(
        [str(AVALON_EXE)],
        cwd=SCRIPT_DIR,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        **kwargs,
    )

    # Wait for backend to print its startup line or bind port
    start = time.time()
    while time.time() - start < 15:
        ret = backend_proc.poll()
        if ret is not None:
            out, _ = backend_proc.communicate()
            log(f"Backend exited early with code {ret}:", level="ERROR")
            if out:
                print(out)
            sys.exit(1)

        if is_port_open("127.0.0.1", BACKEND_PORT, timeout=0.5):
            log(f"Backend is live on http://127.0.0.1:{BACKEND_PORT}")
            return backend_proc
        time.sleep(0.2)

    log("Backend did not start within timeout.", level="ERROR")
    backend_proc.terminate()
    sys.exit(1)


# ---------------------------------------------------------------------------
# GUI (Electron)
# ---------------------------------------------------------------------------

def start_gui():
    client_dir = SCRIPT_DIR / "client"
    pkg_json = client_dir / "package.json"

    if not pkg_json.exists():
        log("No client/package.json found — skipping GUI startup.")
        log("The backend API is running. You can use curl or build the React/Electron app later.")
        return None

    npm_path = shutil.which("npm")
    if not npm_path:
        log("npm not found on PATH — cannot start GUI.", level="WARN")
        return None

    node_modules = client_dir / "node_modules"
    if not node_modules.exists():
        log("Installing client dependencies (npm install)...")
        try:
            if platform.system() == "Windows":
                subprocess.run([npm_path, "install"], cwd=client_dir, check=True, shell=True)
            else:
                subprocess.run([npm_path, "install"], cwd=client_dir, check=True)
        except subprocess.CalledProcessError as e:
            log(f"npm install failed: {e}", level="ERROR")
            return None

    log("Starting Electron GUI...")
    kwargs = {}
    if platform.system() == "Windows":
        kwargs["creationflags"] = subprocess.CREATE_NO_WINDOW
    else:
        kwargs["start_new_session"] = True

    gui_proc = subprocess.Popen(
        [npm_path, "start"],
        cwd=client_dir,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        **kwargs,
    )
    log("GUI launched.")
    return gui_proc


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(description="Avalon Launcher")
    parser.add_argument(
        "mode",
        nargs="?",
        default="local",
        choices=["local", "cloud", "dummy"],
        help="Inference mode: local (default), cloud, or dummy",
    )
    args = parser.parse_args()

    load_dotenv()

    print("=" * 50)
    print("  Avalon Harness Launcher")
    print(f"  Mode: {args.mode}")
    print("=" * 50)
    print()

    if args.mode == "cloud" and not os.environ.get("AVALON_MODEL_API_KEY"):
        log("AVALON_MODEL_API_KEY is not set. Set it in your environment or .env file.", level="ERROR")
        sys.exit(1)

    api_base, model_name = ensure_inference_server(args.mode)
    if api_base:
        log(f"Inference endpoint: {api_base}")
        log(f"Model name:       {model_name}")

    build_backend()
    backend_proc = start_backend(api_base, model_name, args.mode)
    gui_proc = start_gui()

    print()
    log("All systems are up.")
    log("Backend: http://127.0.0.1:8080/v1/infer")
    log("Press Ctrl+C in this window to shut everything down.")
    print()

    try:
        while True:
            ret = backend_proc.poll()
            if ret is not None:
                log(f"Backend exited with code {ret}.")
                break

            if gui_proc and gui_proc.poll() is not None:
                log("GUI closed.")
                log("Shutting down backend...")
                gui_proc = None
                break

            time.sleep(0.5)
    except KeyboardInterrupt:
        log("Shutdown requested.")
    finally:
        log("Stopping backend...")
        try:
            backend_proc.terminate()
            backend_proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            backend_proc.kill()
            backend_proc.wait(timeout=5)
        except Exception:
            pass

        if gui_proc:
            log("Stopping GUI...")
            try:
                gui_proc.terminate()
                gui_proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                gui_proc.kill()
                gui_proc.wait(timeout=5)
            except Exception:
                pass

        log("Goodbye.")
        input("Press Enter to access prompt...")


if __name__ == "__main__":
    main()
