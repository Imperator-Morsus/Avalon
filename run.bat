@echo off
REM Avalon Backend Startup Script (Windows)
REM Usage: run.bat [local|cloud|dummy]

cd /d "%~dp0"

set MODE=%1
if "%MODE%"=="" set MODE=local

echo ========================================
echo   Avalon Backend Launcher
echo   Mode: %MODE%
echo ========================================
echo.

REM --- Environment Setup ---

if exist .env (
    echo Loading environment from .env...
    for /f "usebackq tokens=*" %%a in (`.env`) do set %%a
)

if "%MODE%"=="local" (
    echo Running with LOCAL model settings.
    echo Make sure your local inference server (Ollama, LM Studio, etc.) is already running.
    echo.
    if not defined AVALON_MODEL_API_BASE set AVALON_MODEL_API_BASE=http://localhost:11434/v1
    if not defined AVALON_MODEL_NAME set AVALON_MODEL_NAME=llama3
    echo API Base: %AVALON_MODEL_API_BASE%
    echo Model:    %AVALON_MODEL_NAME%
) else if "%MODE%"=="cloud" (
    echo Running with CLOUD API settings.
    echo.
    if not defined AVALON_MODEL_API_KEY (
        echo ERROR: AVALON_MODEL_API_KEY is not set.
        echo Set it in your environment or in a .env file.
        exit /b 1
    )
    if not defined AVALON_MODEL_API_BASE set AVALON_MODEL_API_BASE=https://api.openai.com/v1
    if not defined AVALON_MODEL_NAME set AVALON_MODEL_NAME=gpt-4o-mini
    echo API Base: %AVALON_MODEL_API_BASE%
    echo Model:    %AVALON_MODEL_NAME%
) else if "%MODE%"=="dummy" (
    echo Running with DUMMY model (no real inference).
    echo.
) else (
    echo Usage: run.bat [local^|cloud^|dummy]
    echo.
    echo   local  - Connect to a local model server (Ollama, LM Studio, etc.)
    echo   cloud  - Connect to a cloud API (OpenAI, etc.)
    echo   dummy  - Use the mock inference service (for testing)
    echo.
    echo You can also create a .env file with your settings.
    exit /b 1
)

echo.
echo Building...
cargo build --release
if errorlevel 1 (
    echo Build failed!
    exit /b 1
)

echo.
echo Starting Avalon Backend...
echo Press Ctrl+C to stop.
echo.

.\target\release\avalon_backend.exe
