'use strict';

const { contextBridge, ipcRenderer } = require('electron');

const ALLOWED_CLIENTS = new Set(['claude', 'codex', 'antigravity']);
const SESSION_DETAIL_CLIENTS = new Set(['claude', 'codex']);
const disabledResult = () => Promise.resolve({ ok: false, disabled: true, reason: 'disabled-by-downstream-policy' });
const allowedClient = (clientId) => ALLOWED_CLIENTS.has(String(clientId || '').trim().toLowerCase());
const allowedSessionClient = (args) => SESSION_DETAIL_CLIENTS.has(String(args?.client || '').trim().toLowerCase());

contextBridge.exposeInMainWorld('tokenMonitor', {
  getSettings: () => ipcRenderer.invoke('settings:get'),
  updateSettings: (patch) => ipcRenderer.invoke('settings:update', patch),
  saveSubscriptions: (subscriptions, base) => ipcRenderer.invoke('subscriptions:save', subscriptions, base),
  adoptOrphanedSubscriptions: () => ipcRenderer.invoke('subscriptions:adoptOrphans'),
  discardOrphanedSubscriptions: () => ipcRenderer.invoke('subscriptions:discardOrphans'),
  clearSessionUsageArchive: () => ipcRenderer.invoke('sessionUsageArchive:clear'),
  lookupModelPricing: (modelId) => ipcRenderer.invoke('pricing:lookup', modelId),
  previewAppearance: (patch) => ipcRenderer.invoke('appearance:preview', patch),
  getStats: (options) => ipcRenderer.invoke('stats:get', options),
  getSessionDetail: (args) => allowedSessionClient(args) ? ipcRenderer.invoke('session:getDetail', args) : disabledResult(),
  getStreamStatus: () => ipcRenderer.invoke('stream:status'),
  getServiceStatus: () => disabledResult(),
  getCodexResetForecast: () => disabledResult(),
  openDashboard: () => ipcRenderer.invoke('dashboard:open'),
  getDashboardHistory: (options) => ipcRenderer.invoke('dashboard:getHistory', options),
  onDashboardHistoryChanged: (callback) => {
    const listener = () => { try { callback(); } catch (_) {} };
    ipcRenderer.on('dashboard:historyChanged', listener);
    return () => ipcRenderer.removeListener('dashboard:historyChanged', listener);
  },
  dashboard: {
    ready: () => ipcRenderer.send('dashboard:ready'),
    minimize: () => ipcRenderer.send('dashboard:minimize'),
    close: () => ipcRenderer.send('dashboard:close')
  },
  getHubInfo: () => ipcRenderer.invoke('hub:getInfo'),
  getHubBuildStatus: () => ipcRenderer.invoke('hub:getBuildStatus'),
  regenerateHubSecret: () => disabledResult(),
  onHubPush: (callback) => {
    const listener = (_event, payload) => { try { callback(payload); } catch (_) {} };
    ipcRenderer.on('hub:push', listener);
    return () => ipcRenderer.removeListener('hub:push', listener);
  },
  onStatsPush: (callback) => {
    const listener = (_event, payload) => { try { callback(payload); } catch (_) {} };
    ipcRenderer.on('stats:push', listener);
    return () => ipcRenderer.removeListener('stats:push', listener);
  },
  onWindowVisibilityPush: (callback) => {
    const listener = (_event, visible) => { try { callback(Boolean(visible)); } catch (_) {} };
    ipcRenderer.on('window:visibility', listener);
    return () => ipcRenderer.removeListener('window:visibility', listener);
  },
  onSettingsPush: (callback) => {
    const listener = (_event, payload) => { try { callback(payload); } catch (_) {} };
    ipcRenderer.on('settings:push', listener);
    return () => ipcRenderer.removeListener('settings:push', listener);
  },
  onSystemUiThemePush: (callback) => {
    const listener = (_event, payload) => { try { callback(payload); } catch (_) {} };
    ipcRenderer.on('theme:systemUi', listener);
    return () => ipcRenderer.removeListener('theme:systemUi', listener);
  },
  onOpenSettings: (callback) => {
    const listener = () => { try { callback(); } catch (_) {} };
    ipcRenderer.on('settings:open', listener);
    return () => ipcRenderer.removeListener('settings:open', listener);
  },
  onOpenView: (callback) => {
    const listener = (_event, viewId) => { try { callback(viewId); } catch (_) {} };
    ipcRenderer.on('view:open', listener);
    return () => ipcRenderer.removeListener('view:open', listener);
  },
  onTokscalePush: (callback) => {
    const listener = (_event, payload) => { try { callback(payload); } catch (_) {} };
    ipcRenderer.on('tokscale:push', listener);
    return () => ipcRenderer.removeListener('tokscale:push', listener);
  },
  getAppInfo: () => ipcRenderer.invoke('app:getInfo'),
  generateDiagnosticReport: () => ipcRenderer.invoke('diagnostics:generate'),
  copyText: (text) => ipcRenderer.invoke('clipboard:write', text),
  clientSources: (clientId) => allowedClient(clientId) ? ipcRenderer.invoke('usage:clientSources', clientId) : disabledResult(),
  revealClientSource: (clientId) => allowedClient(clientId) ? ipcRenderer.invoke('usage:revealClientSource', clientId) : disabledResult(),
  revealClientSyncLock: (clientId) => allowedClient(clientId) ? ipcRenderer.invoke('usage:revealClientSyncLock', clientId) : disabledResult(),
  rescanClient: (clientId) => allowedClient(clientId) ? ipcRenderer.invoke('usage:rescanClient', clientId) : disabledResult(),
  repairClientSyncLock: (clientId) => allowedClient(clientId) ? ipcRenderer.invoke('usage:repairClientSyncLock', clientId) : disabledResult(),
  openExternal: (url) => ipcRenderer.invoke('app:openExternal', url),
  openUserData: () => ipcRenderer.invoke('app:openUserData'),
  antigravity: {
    accounts: () => ipcRenderer.invoke('antigravity:accounts'),
    addAccount: () => disabledResult(),
    cancelLogin: () => disabledResult(),
    removeAccount: () => disabledResult(),
    setAccountEnabled: () => disabledResult(),
    onAccounts: (callback) => {
      const handler = (_event, accounts) => callback(accounts);
      ipcRenderer.on('antigravity:accounts', handler);
      return () => ipcRenderer.removeListener('antigravity:accounts', handler);
    }
  },
  mimo: {
    accounts: () => disabledResult(),
    addAccount: () => disabledResult(),
    openConsole: () => disabledResult(),
    removeAccount: () => disabledResult(),
    setAccountEnabled: () => disabledResult(),
    onAccounts: () => () => {}
  },
  exportNow: () => ipcRenderer.invoke('export:now'),
  pickExportDir: () => ipcRenderer.invoke('export:pickAutoDir'),
  getTokscaleStatus: () => ipcRenderer.invoke('tokscale:getStatus'),
  checkTokscaleNpm: () => disabledResult(),
  downloadTokscaleFromNpm: () => disabledResult(),
  resetTokscaleToBundled: () => ipcRenderer.invoke('tokscale:resetToBundled'),
  getAppUpdateState: () => ipcRenderer.invoke('appUpdate:getState'),
  checkAppUpdateNow: () => disabledResult(),
  downloadAppUpdate: () => disabledResult(),
  installAppUpdate: () => disabledResult(),
  dismissAppUpdate: (version) => ipcRenderer.invoke('appUpdate:dismiss', version),
  expandFloatingBubble: () => ipcRenderer.invoke('floatingBubble:expand'),
  moveFloatingBubble: (delta) => ipcRenderer.invoke('floatingBubble:move', delta),
  signalContentReady: () => ipcRenderer.send('window:contentReady'),
  setViewState: (patch) => ipcRenderer.send('window:viewState', patch),
  peekFloatingBubble: () => ipcRenderer.invoke('floatingBubble:peek'),
  collapseFloatingBubbleIfIdle: () => ipcRenderer.invoke('floatingBubble:collapseIfIdle'),
  setFloatingBubbleCollapsedSize: (size) => ipcRenderer.invoke('floatingBubble:setCollapsedSize', size),
  onFloatingBubbleState: (callback) => {
    const listener = (_event, payload) => { try { callback(payload); } catch (_) {} };
    ipcRenderer.on('floatingBubble:state', listener);
    return () => ipcRenderer.removeListener('floatingBubble:state', listener);
  },
  onAppUpdatePush: (callback) => {
    const listener = (_event, payload) => { try { callback(payload); } catch (_) {} };
    ipcRenderer.on('appUpdate:push', listener);
    return () => ipcRenderer.removeListener('appUpdate:push', listener);
  },
  setTrayIcons: (icons) => ipcRenderer.invoke('tray:setIcons', icons),
  cursor: {
    loginManual: () => disabledResult(),
    setAccountEnabled: () => disabledResult(),
    logout: () => disabledResult(),
    status: () => disabledResult()
  },
  claude: {
    saveCookie: () => disabledResult()
  },
  ollama: {
    validateCookie: () => disabledResult()
  },
  opencode: {
    saveCookie: () => disabledResult(),
    logout: () => disabledResult(),
    status: () => disabledResult(),
    getProfiles: () => disabledResult(),
    saveProfile: () => disabledResult(),
    deleteProfile: () => disabledResult(),
    renameProfile: () => disabledResult(),
    removeCredential: () => disabledResult(),
    moveCredential: () => disabledResult(),
    setProfileEnabled: () => disabledResult(),
    setAmbientEnabled: () => disabledResult()
  },
  openrouter: {
    getProfiles: () => disabledResult(),
    saveProfile: () => disabledResult(),
    deleteProfile: () => disabledResult(),
    renameProfile: () => disabledResult(),
    setProfileEnabled: () => disabledResult()
  },
  thirdparty: {
    getProfiles: () => disabledResult(),
    saveProfile: () => disabledResult(),
    deleteProfile: () => disabledResult(),
    renameProfile: () => disabledResult(),
    setProfileEnabled: () => disabledResult()
  },
  codex: {
    accounts: () => ipcRenderer.invoke('codex:accounts'),
    addAccount: () => disabledResult(),
    selectWorkspace: () => disabledResult(),
    cancelLogin: () => disabledResult(),
    removeAccount: () => disabledResult(),
    setAccountEnabled: () => disabledResult(),
    switchSystemAccount: () => disabledResult(),
    refreshAccountLimits: () => disabledResult(),
    onLoginStatus: (callback) => {
      const handler = (_event, status) => callback(status);
      ipcRenderer.on('codex:loginStatus', handler);
      return () => ipcRenderer.removeListener('codex:loginStatus', handler);
    }
  },
  copilot: {
    signIn: () => disabledResult(),
    cancelSignIn: () => disabledResult(),
    onLoginStatus: () => () => {}
  },
  minimize: () => ipcRenderer.send('window:minimize'),
  close: () => ipcRenderer.send('window:close')
});
