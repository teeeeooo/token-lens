'use strict';

const fs = require('node:fs');
const path = require('node:path');

const target = path.join(__dirname, '..', '..', 'src', 'electron', 'main.js');
let source = fs.readFileSync(target, 'utf8');

function replaceExactlyOnce(before, after, label) {
  if (source.includes(after)) return;
  const first = source.indexOf(before);
  if (first < 0) throw new Error(`downstream patch anchor missing or changed upstream: ${label}`);
  if (source.indexOf(before, first + before.length) >= 0) {
    throw new Error(`downstream patch anchor is ambiguous: ${label}`);
  }
  source = source.slice(0, first) + after + source.slice(first + before.length);
}

replaceExactlyOnce(
  "const { app, BrowserWindow, clipboard, dialog, globalShortcut, ipcMain, nativeImage, nativeTheme, net, Notification, screen, session, shell } = require('electron');",
  "const { app, BrowserWindow, clipboard, dialog, globalShortcut, ipcMain, nativeImage, nativeTheme, net, Notification, screen, session, shell } = require('electron');\n\n// Keep Token Lens runtime state isolated from an upstream Token Monitor install.\n// The explicit userData path also becomes the shared-data root used by archives.\napp.setName('Token Lens');\nconst tokenLensUserData = path.join(app.getPath('appData'), 'Token Lens');\napp.setPath('userData', tokenLensUserData);\nprocess.env.TOKEN_MONITOR_SHARED_DIR = tokenLensUserData;",
  'Token Lens runtime identity'
);

replaceExactlyOnce(
  "const { DEFAULT_CLIENTS, KNOWN_CLIENTS, clientsCsvForSetting } = require('../shared/clientTracking');",
  "const { DEFAULT_CLIENTS, KNOWN_CLIENTS, clientsCsvForSetting } = require('../shared/clientTracking');\nconst { DOWNSTREAM_POLICY, enforceDownstreamSettings } = require('../shared/downstreamPolicy');",
  'downstream policy import'
);

replaceExactlyOnce(
  "  const envHubUrl = process.env.TOKEN_MONITOR_HUB_URL || '';",
  "  // Token Lens is local-only by policy. Ignore upstream Hub environment configuration.\n  const envHubUrl = '';",
  'default Hub URL'
);

replaceExactlyOnce(
  '  settings = readSettings();',
  '  settings = enforceDownstreamSettings(readSettings());',
  'initial settings policy enforcement'
);

replaceExactlyOnce(
  "    }, windowBehaviorSelection(normalizedPatch));\n    settings.archivedClientUsage = normalizeArchivedClientUsage(settings.archivedClientUsage);",
  "    }, windowBehaviorSelection(normalizedPatch));\n    settings = enforceDownstreamSettings(settings);\n    settings.archivedClientUsage = normalizeArchivedClientUsage(settings.archivedClientUsage);",
  'settings update policy enforcement'
);

replaceExactlyOnce(
  "function maybeRunBackgroundUpdateCheck() {\n  runAppUpdateCheck({ force: false }).catch(() => {});\n}",
  "function maybeRunBackgroundUpdateCheck() {\n  if (!DOWNSTREAM_POLICY.appUpdateChecks) return;\n  runAppUpdateCheck({ force: false }).catch(() => {});\n}",
  'background app update checks'
);

replaceExactlyOnce(
  "  ipcMain.handle('tokscale:checkNpm', () => checkTokscaleNpm());\n  ipcMain.handle('tokscale:downloadFromNpm', () => downloadTokscaleFromNpm());",
  "  ipcMain.handle('tokscale:checkNpm', () => DOWNSTREAM_POLICY.runtimeDownloads\n    ? checkTokscaleNpm()\n    : { ok: false, disabled: true, reason: 'disabled-by-downstream-policy' });\n  ipcMain.handle('tokscale:downloadFromNpm', () => DOWNSTREAM_POLICY.runtimeDownloads\n    ? downloadTokscaleFromNpm()\n    : { ok: false, disabled: true, reason: 'disabled-by-downstream-policy' });",
  'runtime tokscale downloads'
);

replaceExactlyOnce(
  "  ipcMain.handle('appUpdate:checkNow', () => runAppUpdateCheck({ force: true }));\n  ipcMain.handle('appUpdate:download', () => downloadAndPrepareAppUpdate());\n  ipcMain.handle('appUpdate:install', () => installDownloadedAppUpdate());",
  "  ipcMain.handle('appUpdate:checkNow', () => DOWNSTREAM_POLICY.appUpdateChecks\n    ? runAppUpdateCheck({ force: true })\n    : { ok: false, disabled: true, reason: 'disabled-by-downstream-policy' });\n  ipcMain.handle('appUpdate:download', () => DOWNSTREAM_POLICY.appUpdates\n    ? downloadAndPrepareAppUpdate()\n    : { ok: false, disabled: true, reason: 'disabled-by-downstream-policy' });\n  ipcMain.handle('appUpdate:install', () => DOWNSTREAM_POLICY.appUpdates\n    ? installDownloadedAppUpdate()\n    : { ok: false, disabled: true, reason: 'disabled-by-downstream-policy' });",
  'app update IPC'
);

replaceExactlyOnce(
  "  ipcMain.handle('hub:regenerateSecret', () => {\n    settings.hubHostSecret = generateHubSecret();",
  "  ipcMain.handle('hub:regenerateSecret', () => {\n    if (!DOWNSTREAM_POLICY.hub) return { ok: false, disabled: true, reason: 'disabled-by-downstream-policy' };\n    settings.hubHostSecret = generateHubSecret();",
  'Hub secret IPC'
);

replaceExactlyOnce(
  "  ipcMain.handle('claude:saveCookie', async (_event, raw) => {\n    const requestRevision = ++claudeWebCookieMutationRevision;",
  "  ipcMain.handle('claude:saveCookie', async (_event, raw) => {\n    if (!DOWNSTREAM_POLICY.claudeWebCookie) return { ok: false, disabled: true, reason: 'disabled-by-downstream-policy' };\n    const requestRevision = ++claudeWebCookieMutationRevision;",
  'Claude Web cookie IPC'
);

fs.writeFileSync(target, source);
console.log('Token Lens downstream hardening is applied to src/electron/main.js');
