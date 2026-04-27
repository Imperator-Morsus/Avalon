const { app, BrowserWindow } = require('electron');
const path = require('path');
const { spawn, exec } = require('child_process');

let backendProcess = null;

function startBackend() {
  const exePath = path.join(__dirname, '..', 'target', 'release', 'avalon_backend.exe');
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

const createWindow = () => {
  const win = new BrowserWindow({
    width: 1400,
    height: 900,
    minWidth: 900,
    minHeight: 600,
    title: 'Avalon',
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
    },
  });

  win.loadFile(path.join(__dirname, 'ui', 'index.html'));

  // Open DevTools in development
  if (process.argv.includes('--dev')) {
    win.webContents.openDevTools();
  }
};

app.whenReady().then(() => {
  startBackend();
  createWindow();

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow();
  });
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit();
});

app.on('before-quit', () => {
  stopBackend();
});
