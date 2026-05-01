#Requires -RunAsAdministrator
<#
.SYNOPSIS
    Moves C:\Users\Worldsmith\.claude to D:\ClaudeData and creates a directory junction.
.DESCRIPTION
    This script safely relocates Claude Code's data directory from C:\ to D:\,
    then creates a junction so Claude Code continues to work transparently.
    Run this while Claude Code is CLOSED.
#>

$ErrorActionPreference = "Stop"

$source      = "C:\Users\Worldsmith\.claude"
$destParent  = "D:\ClaudeData"
$dest        = "$destParent\.claude"
$junction    = $source
$backupZip   = "$destParent\.claude-backup-$(Get-Date -Format 'yyyyMMdd-HHmmss').zip"

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  Claude Code Data Migration Script" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# ── 1. Check for running Claude Code processes ────────────────────────────────
$claudeProcs = Get-Process -Name "claude" -ErrorAction SilentlyContinue
if ($claudeProcs) {
    Write-Warning "Claude Code appears to be running. Please close it before proceeding."
    Write-Host "Detected processes:"
    $claudeProcs | ForEach-Object { Write-Host "  - $($_.ProcessName) (PID: $($_.Id))" }
    Write-Host ""
    $cont = Read-Host "Close Claude Code and press ENTER to continue, or type N to abort"
    if ($cont -match "^[Nn]") { exit 1 }
}

# Double-check after pause
$claudeProcs = Get-Process -Name "claude" -ErrorAction SilentlyContinue
if ($claudeProcs) {
    Write-Error "Claude Code is still running. Aborting."
    exit 1
}

# ── 2. Validate source exists ─────────────────────────────────────────────────
if (-not (Test-Path $source)) {
    Write-Error "Source directory does not exist: $source"
    exit 1
}

Write-Host "Source:      $source" -ForegroundColor Yellow
Write-Host "Destination: $dest" -ForegroundColor Yellow
Write-Host "Junction:    $junction" -ForegroundColor Yellow
Write-Host ""

# ── 3. Create destination parent if needed ──────────────────────────────────────
if (-not (Test-Path $destParent)) {
    Write-Host "Creating destination parent directory: $destParent"
    New-Item -ItemType Directory -Path $destParent -Force | Out-Null
}

# ── 4. Backup existing data ───────────────────────────────────────────────────
Write-Host "Creating backup zip: $backupZip" -ForegroundColor Green
Compress-Archive -Path "$source\*" -DestinationPath $backupZip -Force
Write-Host "Backup complete." -ForegroundColor Green
Write-Host ""

# ── 5. Remove old junction if it exists (from a previous run) ─────────────────
if (Test-Path $junction) {
    $item = Get-Item $junction
    if ($item.Attributes -match "ReparsePoint") {
        Write-Host "Removing stale junction at $junction"
        Remove-Item $junction -Force
    } else {
        Write-Error "$junction exists and is not a junction. Aborting to prevent data loss."
        exit 1
    }
}

# ── 6. Move the directory ─────────────────────────────────────────────────────
Write-Host "Moving data from C:\ to D:\ ..." -ForegroundColor Green
if (Test-Path $dest) {
    Write-Warning "Destination already exists. Renaming old folder to .claude-old-$(Get-Date -Format 'yyyyMMdd-HHmmss')"
    Rename-Item $dest "$dest-old-$(Get-Date -Format 'yyyyMMdd-HHmmss')"
}
Move-Item -Path $source -Destination $dest -Force
Write-Host "Move complete." -ForegroundColor Green
Write-Host ""

# ── 7. Create the junction ────────────────────────────────────────────────────
Write-Host "Creating directory junction..." -ForegroundColor Green
cmd /c mklink /J "$junction" "$dest" | Out-Null
if ($LASTEXITCODE -ne 0) {
    Write-Error "Failed to create junction. Attempting rollback..."
    Move-Item -Path $dest -Destination $source -Force
    Write-Host "Rolled back. Data is back at $source"
    exit 1
}
Write-Host "Junction created successfully." -ForegroundColor Green
Write-Host ""

# ── 8. Verify ─────────────────────────────────────────────────────────────────
Write-Host "Verifying..." -ForegroundColor Cyan
$junctionItem = Get-Item $junction
if ($junctionItem.Attributes -match "ReparsePoint") {
    Write-Host "OK: $junction is a reparse point (junction)." -ForegroundColor Green
} else {
    Write-Warning "Warning: $junction does not appear to be a junction."
}

$testFile = Join-Path $junction "migration-test.txt"
"Migration verified on $(Get-Date)" | Out-File -FilePath $testFile -Encoding UTF8
$destTestFile = Join-Path $dest "migration-test.txt"
if (Test-Path $destTestFile) {
    Write-Host "OK: Write through junction reached destination on D:\." -ForegroundColor Green
    Remove-Item $destTestFile
} else {
    Write-Warning "Write test did not propagate to destination."
}

# ── 9. Summary ────────────────────────────────────────────────────────────────
Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  Migration Complete!" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "Data now lives at: $dest" -ForegroundColor Yellow
Write-Host "Junction at:       $junction" -ForegroundColor Yellow
Write-Host "Backup saved to:   $backupZip" -ForegroundColor Yellow
Write-Host ""
Write-Host "You can now reopen Claude Code. All future data will be stored on D:\." -ForegroundColor Green
Write-Host ""
Write-Host "To REVERSE this later, run:" -ForegroundColor DarkGray
Write-Host "  Remove-Item '$junction' -Force" -ForegroundColor DarkGray
Write-Host "  Move-Item '$dest' '$source'" -ForegroundColor DarkGray
Write-Host ""

Read-Host "Press ENTER to exit"
