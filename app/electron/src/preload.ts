import { contextBridge, ipcRenderer } from 'electron';

const unavailable = () => Promise.reject(new Error('not available yet'));
contextBridge.exposeInMainWorld('openTouchDeck', {
  version: 1,
  session: (): Promise<string | null> => ipcRenderer.invoke('session'),
  onSessionChanged: (callback: (token: string) => void) => {
    if (typeof callback !== 'function') throw new TypeError('Expected a callback');
    const listener = (_event: Electron.IpcRendererEvent, token: string) => callback(token);
    ipcRenderer.on('session-changed', listener);
    return () => ipcRenderer.removeListener('session-changed', listener);
  },
  updates: { check: unavailable, download: unavailable, install: unavailable },
});
