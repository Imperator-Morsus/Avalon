#!/bin/bash
# Avalon Backend Startup Script
# Usage: ./run.sh [local|cloud|dummy]

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

MODE="${1:-local}"

echo "========================================"
echo "  Avalon Backend Launcher"
echo "  Mode: $MODE"
echo "========================================"
echo ""

# --- Environment Setup ---

if [ -f .env ]; then
    echo "Loading environment from .env..."
    export $(grep -v '^#' .env | xargs)
fi

case "$MODE" in
    local)
        echo "Running with LOCAL model settings."
        echo "Make sure your local inference server (Ollama, LM Studio, etc.) is already running."
        echo ""
        # Default local settings if not already set
        export AVALON_MODEL_API_BASE="${AVALON_MODEL_API_BASE:-http://localhost:11434/v1}"
        export AVALON_MODEL_NAME="${AVALON_MODEL_NAME:-llama3}"
        echo "API Base: $AVALON_MODEL_API_BASE"
        echo "Model:    $AVALON_MODEL_NAME"
        ;;
    cloud)
        echo "Running with CLOUD API settings."
        echo ""
        if [ -z "$AVALON_MODEL_API_KEY" ]; then
            echo "ERROR: AVALON_MODEL_API_KEY is not set."
            echo "Set it in your environment or in a .env file."
            exit 1
        fi
        export AVALON_MODEL_API_BASE="${AVALON_MODEL_API_BASE:-https://api.openai.com/v1}"
        export AVALON_MODEL_NAME="${AVALON_MODEL_NAME:-gpt-4o-mini}"
        echo "API Base: $AVALON_MODEL_API_BASE"
        echo "Model:    $AVALON_MODEL_NAME"
        ;;
    dummy)
        echo "Running with DUMMY model (no real inference)."
        echo ""
        ;;
    *)
        echo "Usage: ./run.sh [local|cloud|dummy]"
        echo ""
        echo "  local  - Connect to a local model server (Ollama, LM Studio, etc.)"
        echo "  cloud  - Connect to a cloud API (OpenAI, etc.)"
        echo "  dummy  - Use the mock inference service (for testing)"
        echo ""
        echo "You can also create a .env file with your settings."
        exit 1
        ;;
esac

echo ""
echo "Building..."
cargo build --release

echo ""
echo "Starting Avalon Backend..."
echo "Press Ctrl+C to stop."
echo ""

if [ "$MODE" = "dummy" ]; then
    ./target/release/avalon_backend.exe
else
    ./target/release/avalon_backend.exe
fi
