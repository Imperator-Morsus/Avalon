const { contextBridge, ipcRenderer } = require('electron');

// Minimal preload: window controls need IPC because contextIsolation is on.
contextBridge.exposeInMainWorld('avalon', {
  version: '0.3.0',
  windowMinimize: () => ipcRenderer.send('window-minimize'),
  windowMaximize: () => ipcRenderer.send('window-maximize'),
  windowClose:    () => ipcRenderer.send('window-close'),
  restartBackend: () => ipcRenderer.send('restart-backend'),
  // Auth IPC — renderer invokes main process to call backend
  authLogin:  (username, password) => ipcRenderer.invoke('auth-login', username, password),
  authLogout: ()                   => ipcRenderer.invoke('auth-logout'),
  authMe:    ()                    => ipcRenderer.invoke('auth-me'),
  authGetToken: ()                => ipcRenderer.invoke('auth-get-token'),
});
