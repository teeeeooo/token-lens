'use strict';

const fs = require('node:fs');
const path = require('node:path');

const root = path.join(__dirname, '..', '..');
const materializedFiles = [];

function patchFile(relativePath, applyPatches) {
  const target = path.join(root, relativePath);
  let source = fs.readFileSync(target, 'utf8');
  let changed = false;

  function replaceExactlyOnce(before, after, label) {
    if (source.includes(after)) return;
    const first = source.indexOf(before);
    if (first < 0) throw new Error(`downstream patch anchor missing or changed upstream: ${relativePath}: ${label}`);
    if (source.indexOf(before, first + before.length) >= 0) {
      throw new Error(`downstream patch anchor is ambiguous: ${relativePath}: ${label}`);
    }
    source = source.slice(0, first) + after + source.slice(first + before.length);
    changed = true;
  }

  applyPatches(replaceExactlyOnce);
  if (changed) {
    fs.writeFileSync(target, source);
    materializedFiles.push(relativePath);
  }
}

patchFile('src/electron/main.js', (replaceExactlyOnce) => {
  replaceExactlyOnce(
    "const { app, BrowserWindow, clipboard, dialog, globalShortcut, ipcMain, nativeImage, nativeTheme, net, Notification, screen, session, shell } = require('electron');",
    "const { app, BrowserWindow, clipboard, dialog, globalShortcut, ipcMain, nativeImage, nativeTheme, net, Notification, screen, session, shell } = require('electron');\n\n// Keep Token Lens runtime state isolated from an upstream Token Monitor install.\n// The explicit userData path also becomes the shared-data root used by archives.\napp.setName('Token Lens');\nconst tokenLensUserData = path.join(app.getPath('appData'), 'Token Lens');\napp.setPath('userData', tokenLensUserData);\nprocess.env.TOKEN_MONITOR_SHARED_DIR = tokenLensUserData;",
    'Token Lens runtime identity'
  );
  replaceExactlyOnce("const APP_NAME = 'Token Monitor';", "const APP_NAME = 'Token Lens';", 'application name');
  replaceExactlyOnce(
    "if (process.platform === 'win32') app.setAppUserModelId('com.javis.tokenmonitor');",
    "if (process.platform === 'win32') app.setAppUserModelId('com.teeeeooo.tokenlens');",
    'Windows AppUserModelId'
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
  replaceExactlyOnce('  settings = readSettings();', '  settings = enforceDownstreamSettings(readSettings());', 'initial settings policy enforcement');
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
});

patchFile('src/shared/credentialStore.js', (replaceExactlyOnce) => {
  replaceExactlyOnce(
    "const path = require('node:path');",
    "const path = require('node:path');\nconst { DOWNSTREAM_POLICY } = require('./downstreamPolicy');",
    'credential policy import'
  );
  replaceExactlyOnce(
    "  readDocument() {\n    let raw;",
    "  readDocument() {\n    if (!DOWNSTREAM_POLICY.credentialPersistence) return emptyDocument();\n    let raw;",
    'credential read gate'
  );
  replaceExactlyOnce(
    "  writeDocument(document) {\n    const normalized = normalizeDocument(document);",
    "  writeDocument(document) {\n    if (!DOWNSTREAM_POLICY.credentialPersistence) return emptyDocument();\n    const normalized = normalizeDocument(document);",
    'credential write gate'
  );
  replaceExactlyOnce(
    "  migrateLegacySettings(legacySettings) {\n    const document = this.readDocument();",
    "  migrateLegacySettings(legacySettings) {\n    if (!DOWNSTREAM_POLICY.credentialPersistence) return { migrated: false, document: emptyDocument() };\n    const document = this.readDocument();",
    'legacy credential migration gate'
  );
  replaceExactlyOnce(
    "  replaceSettingsCredentials(settings, baseDocument = this.readDocument()) {\n    const document = normalizeDocument(baseDocument);",
    "  replaceSettingsCredentials(settings, baseDocument = this.readDocument()) {\n    if (!DOWNSTREAM_POLICY.credentialPersistence) return emptyDocument();\n    const document = normalizeDocument(baseDocument);",
    'settings credential persistence gate'
  );
  replaceExactlyOnce(
    "  readAntigravityCredential(id, document = this.readDocument()) {\n    const accountId = safeDynamicKey(id);",
    "  readAntigravityCredential(id, document = this.readDocument()) {\n    if (!DOWNSTREAM_POLICY.credentialPersistence) return null;\n    const accountId = safeDynamicKey(id);",
    'Antigravity credential read gate'
  );
  replaceExactlyOnce(
    "  writeAntigravityCredential(id, credentials) {\n    const accountId = safeDynamicKey(id);",
    "  writeAntigravityCredential(id, credentials) {\n    if (!DOWNSTREAM_POLICY.credentialPersistence) return false;\n    const accountId = safeDynamicKey(id);",
    'Antigravity credential write gate'
  );
  replaceExactlyOnce(
    "  removeAntigravityCredential(id) {\n    const accountId = safeDynamicKey(id);",
    "  removeAntigravityCredential(id) {\n    if (!DOWNSTREAM_POLICY.credentialPersistence) return false;\n    const accountId = safeDynamicKey(id);",
    'Antigravity credential removal gate'
  );
});

patchFile('src/shared/codexSystemSwitch.js', (replaceExactlyOnce) => {
  replaceExactlyOnce(
    "const { authWithSelectedCodexWorkspace, normalizeWorkspaceId } = require('./codexWorkspaces');",
    "const { authWithSelectedCodexWorkspace, normalizeWorkspaceId } = require('./codexWorkspaces');\nconst { DOWNSTREAM_POLICY } = require('./downstreamPolicy');",
    'Codex credential policy import'
  );
  replaceExactlyOnce(
    "async function writeCodexAuthFile(authPath, data, deps = {}) {\n  const mkdir = deps.mkdir || fs.promises.mkdir;",
    "async function writeCodexAuthFile(authPath, data, deps = {}) {\n  if (!DOWNSTREAM_POLICY.credentialMutation) {\n    const error = new Error('Token Lens treats Codex authentication as read-only.');\n    error.code = 'TOKEN_LENS_READ_ONLY_AUTH';\n    throw error;\n  }\n  const mkdir = deps.mkdir || fs.promises.mkdir;",
    'Codex auth write gate'
  );
});

patchFile('src/electron/antigravityOAuthLogin.js', (replaceExactlyOnce) => {
  replaceExactlyOnce(
    "const antigravityOAuth = require('../shared/antigravityOAuth');",
    "const antigravityOAuth = require('../shared/antigravityOAuth');\nconst { DOWNSTREAM_POLICY } = require('../shared/downstreamPolicy');",
    'Antigravity login policy import'
  );
  replaceExactlyOnce(
    "async function runAntigravityOAuthLogin(options = {}) {\n  const client = options.client || antigravityOAuth.discoverOAuthClient({",
    "async function runAntigravityOAuthLogin(options = {}) {\n  if (!DOWNSTREAM_POLICY.managedAccountLogin) {\n    throw loginError('DISABLED_BY_DOWNSTREAM_POLICY', 'Token Lens uses existing local Antigravity authentication read-only.');\n  }\n  const client = options.client || antigravityOAuth.discoverOAuthClient({",
    'Antigravity managed login gate'
  );
});

patchFile('src/shared/sessionDetail.js', (replaceExactlyOnce) => {
  replaceExactlyOnce(
    "const { readReasonixSessionEvents } = require('./reasonixSessionDetail');",
    "const { readReasonixSessionEvents } = require('./reasonixSessionDetail');\n\nconst TOKEN_LENS_SESSION_CLIENTS = new Set(['claude', 'codex']);",
    'session detail allowlist'
  );
  replaceExactlyOnce(
    "function claudePromptText(content) {\n  if (typeof content === 'string') {\n    if (isSyntheticClaudePrompt(content)) return null;\n    return cleanPromptText(content) || null; // empty / image-ref-only string → skip boundary\n  }\n  if (Array.isArray(content)) {\n    if (content.some((part) => part && part.type === 'tool_result')) return null; // tool output, not a prompt\n    const rawTexts = content.filter((part) => part && part.type === 'text').map((part) => String(part.text || ''));\n    if (rawTexts.some(isSyntheticClaudePrompt)) return null;\n    const joined = rawTexts.map(cleanPromptText).filter(Boolean).join(' ').replace(/\\s+/g, ' ').trim();\n    if (joined) return joined;\n    // No text once the \"[Image: source: …]\" duplicate refs are gone:\n    //   has an image part → genuine image-only prompt → keep a labelled row\n    //   otherwise → text-only paste duplicate → skip so its turns fold into the real prompt\n    return content.some((part) => part && part.type === 'image') ? '[image]' : null;\n  }\n  return null;\n}",
    "function claudePromptText(content) {\n  // Metadata-only boundary detection: inspect structural block types, never user text.\n  if (Array.isArray(content) && content.some((part) => part && part.type === 'tool_result')) return null;\n  if (isSyntheticClaudePrompt('')) return null;\n  return cleanPromptText('[prompt]');\n}",
    'Claude metadata-only prompt parsing'
  );
  replaceExactlyOnce(
    "function codexPromptText(raw) {\n  const text = String(raw || '');\n  const marker = '## My request for Codex:';\n  const idx = text.indexOf(marker);\n  return cleanPromptText(idx >= 0 ? text.slice(idx + marker.length) : text);\n}",
    "function codexPromptText() {\n  // Preserve prompt boundaries without extracting or retaining prompt content.\n  return cleanPromptText('[prompt]');\n}",
    'Codex metadata-only prompt parsing'
  );
  replaceExactlyOnce(
    "function newExchange(promptPreview, timestamp) {\n  return {\n    promptPreview,",
    "function newExchange(promptPreview, timestamp) {\n  void promptPreview;\n  return {\n    promptPreview: '',",
    'prompt preview suppression at source'
  );
  replaceExactlyOnce(
    "function readSessionDetail({ client, sessionId, period = 'total', sessionCost = 0, home, env, useEnvRoots, deps = {} }) {\n  if (client === 'opencode') return readOpenCodeSessionDetail({ sessionId, period, deps });",
    "function readSessionDetail({ client, sessionId, period = 'total', sessionCost = 0, home, env, useEnvRoots, deps = {} }) {\n  if (!TOKEN_LENS_SESSION_CLIENTS.has(client)) {\n    return { found: false, client, sessionId, period, exchanges: [], totals: totalsOf([], sessionCost) };\n  }\n  if (client === 'opencode') return readOpenCodeSessionDetail({ sessionId, period, deps });",
    'session detail provider gate'
  );
});

patchFile('src/shared/exchangeRates.js', (replaceExactlyOnce) => {
  replaceExactlyOnce(
    "const { CURRENCY_CODES } = require('./currency');",
    "const { CURRENCY_CODES } = require('./currency');\nconst { DOWNSTREAM_POLICY } = require('./downstreamPolicy');",
    'ancillary network policy import'
  );
  replaceExactlyOnce(
    "async function fetchRates({ fetchImpl = globalThis.fetch, timeoutMs = 8000, sources = SOURCES } = {}) {\n  let lastErr = null;",
    "async function fetchRates({ fetchImpl = globalThis.fetch, timeoutMs = 8000, sources = SOURCES } = {}) {\n  if (!DOWNSTREAM_POLICY.ancillaryNetwork) throw new Error('disabled-by-downstream-policy');\n  let lastErr = null;",
    'exchange-rate network gate'
  );
});

console.log(materializedFiles.length
  ? `Token Lens downstream hardening materialized: ${materializedFiles.join(', ')}`
  : 'Token Lens downstream hardening is already materialized');
