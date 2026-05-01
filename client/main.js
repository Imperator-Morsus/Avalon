const { app, BrowserWindow, Menu, ipcMain } = require('electron');
const path = require('path');
const { spawn, execSync } = require('child_process');

let backendProcess = null;

function findBackendExe() {
  const releasePath = path.join(__dirname, '..', 'target', 'release', 'avalon_backend.exe');
  const debugPath = path.join(__dirname, '..', 'target', 'debug', 'avalon_backend.exe');
  const fs = require('fs');
  if (fs.existsSync(releasePath)) return releasePath;
  if (fs.existsSync(debugPath)) return debugPath;
  return null;
}

function startBackend() {
  const exePath = findBackendExe();
  if (!exePath) {
    console.error('[Avalon Backend] Not found. Please run: cargo build --release');
    return;
  }

  backendProcess = spawn(exePath, [], {
    detached: false,
    windowsHide: true,
  });

  backendProcess.stdout.on('data', (data) => {
    console.log(`[Avalon Backend] ${data.toString().trim()}`);
  });

  backendProcess.stderr.on('data', (data) => {
    console.error(`[Avalon Backend] ${data.toString().trim()}`);
  });

  backendProcess.on('error', (err) => {
    console.error('Failed to start Avalon backend:', err.message);
  });

  backendProcess.on('exit', (code) => {
    console.log(`Avalon backend exited with code ${code}`);
    backendProcess = null;
  });
}

function stopBackend() {
  return new Promise((resolve) => {
    if (!backendProcess) {
      resolve();
      return;
    }
    const pid = backendProcess.pid;

    backendProcess.on('exit', () => {
      backendProcess = null;
      resolve();
    });

    if (process.platform === 'win32') {
      try {
        execSync(`taskkill /PID ${pid} /T /F`, { stdio: 'ignore' });
      } catch (e) {
        // Process may already be dead
      }
    } else {
      try {
        process.kill(pid, 'SIGTERM');
      } catch (e) {
        // Process may already be dead
      }
    }

    // Timeout fallback
    setTimeout(() => {
      backendProcess = null;
      resolve();
    }, 3000);
  });
}

let mainWindow = null;

const createWindow = () => {
  const win = new BrowserWindow({
    width: 1400,
    height: 900,
    minWidth: 900,
    minHeight: 600,
    title: 'Avalon',
    frame: false,
    roundedCorners: false,
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
    },
  });

  mainWindow = win;

  win.loadFile(path.join(__dirname, 'ui', 'index.html'));

  // Open DevTools in development
  if (process.argv.includes('--dev')) {
    win.webContents.openDevTools();
  }

  win.on('closed', () => {
    mainWindow = null;
  });

  win.on('close', async (e) => {
    e.preventDefault();
    await stopBackend();
    if (mainWindow) mainWindow.destroy();
  });
};

app.whenReady().then(() => {
  startBackend();
  createWindow();
  Menu.setApplicationMenu(null);

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow();
  });
});

app.on('window-all-closed', async () => {
  await stopBackend();
  if (process.platform !== 'darwin') app.quit();
});

app.on('will-quit', async () => {
  await stopBackend();
});

// Backend restart IPC
ipcMain.on('restart-backend', async () => {
  console.log('[Avalon] Restarting backend...');
  await stopBackend();
  setTimeout(() => {
    startBackend();
    console.log('[Avalon] Backend restarted.');
  }, 1500);
});

// Window control IPC
ipcMain.on('window-minimize', () => {
  if (mainWindow) mainWindow.minimize();
});

ipcMain.on('window-maximize', () => {
  if (mainWindow) {
    if (mainWindow.isMaximized()) {
      mainWindow.unmaximize();
    } else {
      mainWindow.maximize();
    }
  }
});

ipcMain.on('window-close', () => {
  stopBackend();
  if (mainWindow) mainWindow.close();
});

// Auth IPC handlers — relay auth operations from renderer to backend
const API_BASE = 'http://127.0.0.1:8080';

ipcMain.handle('auth-login', async (event, username, password) => {
  try {
    const resp = await fetch(`${API_BASE}/api/auth/login`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ username, password })
    });
    return await resp.json();
  } catch(e) {
    return { ok: false, error: 'Connection failed: ' + e.message };
  }
});

ipcMain.handle('auth-logout', async () => {
  try {
    const { sessionStorage } = require('electron').session || {};
    // Get token from sessionStorage — main process can't access renderer sessionStorage directly
    // So we rely on the renderer to call logout with the token in header
    // This just returns ok — actual logout happens via fetch from renderer
    return { ok: true };
  } catch(e) {
    return { ok: false, error: e.message };
  }
});

ipcMain.handle('auth-me', async () => {
  try {
    const resp = await fetch(`${API_BASE}/api/auth/me`);
    return resp.ok ? await resp.json() : { ok: false, error: 'not authenticated' };
  } catch(e) {
    return { ok: false, error: 'Connection failed: ' + e.message };
  }
});

ipcMain.handle('auth-get-token', async () => {
  // Token lives in renderer sessionStorage — main process can't access it directly
  // Return null — renderer uses its own sessionStorage access
  return null;
});
