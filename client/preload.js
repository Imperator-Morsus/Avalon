const { contextBridge, ipcRenderer } = require('electron');

// Minimal preload: window controls need IPC because contextIsolation is on.
contextBridge.exposeInMainWorld('avalon', {
  version: '0.3.0',
  windowMinimize: () => ipcRenderer.send('window-minimize'),
  windowMaximize: () => ipcRenderer.send('window-maximize'),
  windowClose:    () => ipcRenderer.send('window-close'),
});
