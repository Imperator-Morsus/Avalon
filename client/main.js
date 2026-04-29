const { app, BrowserWindow, Menu, ipcMain } = require('electron');
const path = require('path');
const { spawn, exec } = require('child_process');

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
  if (!backendProcess) return;
  const pid = backendProcess.pid;
  backendProcess = null;

  if (process.platform === 'win32') {
    exec(`taskkill /PID ${pid} /T /F`, (err) => {
      if (err) console.error('Error killing backend:', err.message);
      else console.log('Backend stopped.');
    });
  } else {
    try {
      process.kill(pid, 'SIGTERM');
    } catch (e) {
      console.error('Error killing backend:', e.message);
    }
  }
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
    stopBackend();
    mainWindow = null;
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

app.on('window-all-closed', () => {
  stopBackend();
  if (process.platform !== 'darwin') app.quit();
});

app.on('before-quit', () => {
  stopBackend();
});

app.on('will-quit', () => {
  stopBackend();
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
