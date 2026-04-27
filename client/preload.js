const { contextBridge } = require('electron');

// Minimal preload: no special APIs needed since the UI talks directly
// to the backend via fetch() to localhost:8080.
contextBridge.exposeInMainWorld('avalon', {
  version: '0.2.0',
});
