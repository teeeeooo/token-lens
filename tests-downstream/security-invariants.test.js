'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const test = require('node:test');

const {
  ALLOWED_CLIENTS,
  ALLOWED_LIMIT_PROVIDERS,
  DOWNSTREAM_POLICY,
  enforceDownstreamSettings
} = require('../src/shared/downstreamPolicy');
const { DEFAULT_CLIENTS, KNOWN_CLIENTS, clientsCsvForSetting } = require('../src/shared/clientTracking');
const { LIMIT_PROVIDER_IDS } = require('../src/shared/limitProviders');
const {
  CredentialStore,
  persistSettingsAndCredentials
} = require('../src/shared/credentialStore');
const { writeCodexAuthFile } = require('../src/shared/codexSystemSwitch');
const {
  parseClaudeTranscript,
  parseCodexTranscript,
  groupEvents,
  readSessionDetail
} = require('../src/shared/sessionDetail');

const repoRoot = path.join(__dirname, '..');
const readRepo = (relativePath) => fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');

test('provider surface is exactly Claude, Codex, and Antigravity', () => {
  assert.deepEqual(ALLOWED_CLIENTS, ['claude', 'codex', 'antigravity']);
  assert.deepEqual(ALLOWED_LIMIT_PROVIDERS, ['claude', 'codex', 'antigravity']);
  assert.equal(DEFAULT_CLIENTS, 'claude,codex,antigravity');
  assert.equal(KNOWN_CLIENTS, DEFAULT_CLIENTS);
  assert.deepEqual(LIMIT_PROVIDER_IDS, ALLOWED_LIMIT_PROVIDERS);
  assert.equal(clientsCsvForSetting('cursor,codex,claude,evil,antigravity'), DEFAULT_CLIENTS.split(',').sort().join(',').replace('antigravity,claude,codex', 'codex,claude,antigravity'));
});

test('settings are fail-closed for sync, updates, credentials, and unsupported providers', () => {
  const hardened = enforceDownstreamSettings({
    hubMode: 'host',
    hubUrl: 'http://example.invalid',
    hubHostSecret: 'secret',
    secret: 'secret',
    automaticAppUpdates: true,
    discordRpcEnabled: true,
    claudeWebCookie: 'cookie',
    deepseekApiKey: 'key',
    codexManagedAccounts: [{ id: 'x' }],
    antigravityManagedAccounts: [{ id: 'y' }],
    clients: 'cursor,codex,claude,antigravity',
    limitProviders: 'cursor,codex,claude,antigravity'
  });
  assert.equal(hardened.hubMode, 'local');
  assert.equal(hardened.hubUrl, '');
  assert.equal(hardened.hubHostSecret, '');
  assert.equal(hardened.secret, '');
  assert.equal(hardened.automaticAppUpdates, false);
  assert.equal(hardened.discordRpcEnabled, false);
  assert.equal(hardened.claudeWebCookie, '');
  assert.equal(Object.hasOwn(hardened, 'deepseekApiKey'), false);
  assert.deepEqual(hardened.codexManagedAccounts, []);
  assert.deepEqual(hardened.antigravityManagedAccounts, []);
  assert.equal(hardened.clients, 'codex,claude,antigravity');
  assert.equal(hardened.limitProviders, 'codex,claude,antigravity');
  assert.equal(DOWNSTREAM_POLICY.credentialPersistence, false);
  assert.equal(DOWNSTREAM_POLICY.credentialMutation, false);
  assert.equal(DOWNSTREAM_POLICY.ancillaryNetwork, false);
});

test('Token Lens credential store never creates or reads a custom credential document', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'token-lens-credentials-'));
  try {
    const store = new CredentialStore(dir);
    const settingsPath = path.join(dir, 'settings.json');
    const credentialsPath = path.join(dir, 'credentials.json');
    assert.deepEqual(store.readDocument().credentials, {});
    assert.equal(store.writeAntigravityCredential('account', { accessToken: 'secret' }), false);
    persistSettingsAndCredentials({
      store,
      settingsPath,
      settings: { clients: 'codex', deepseekApiKey: 'must-not-persist' },
      previousSettings: {}
    });
    assert.equal(fs.existsSync(credentialsPath), false);
    assert.equal(fs.readFileSync(settingsPath, 'utf8').includes('must-not-persist'), false);

    fs.writeFileSync(credentialsPath, JSON.stringify({
      version: 1,
      credentials: { providers: { antigravity: { accounts: { x: { credentials: { accessToken: 'legacy' } } } } } },
      migrations: {}
    }));
    assert.deepEqual(store.readDocument().credentials, {});
    assert.equal(store.readAntigravityCredential('x'), null);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test('Codex auth mutation is rejected before filesystem writes', async () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'token-lens-codex-'));
  const authPath = path.join(dir, '.codex', 'auth.json');
  try {
    await assert.rejects(
      writeCodexAuthFile(authPath, '{"token":"secret"}\n'),
      (error) => error?.code === 'TOKEN_LENS_READ_ONLY_AUTH'
    );
    assert.equal(fs.existsSync(authPath), false);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test('Claude session detail keeps token metadata but never prompt or response content', () => {
  const prompt = 'SUPER_SECRET_CLAUDE_PROMPT';
  const response = 'SUPER_SECRET_CLAUDE_RESPONSE';
  const transcript = [
    JSON.stringify({ type: 'user', timestamp: '2026-09-04T00:00:00Z', message: { content: prompt } }),
    JSON.stringify({
      type: 'assistant', timestamp: '2026-09-04T00:00:01Z',
      message: { id: 'm1', usage: { input_tokens: 10, output_tokens: 4, cache_read_input_tokens: 2, cache_creation_input_tokens: 1 }, content: [{ type: 'text', text: response }] }
    })
  ].join('\n');
  const events = parseClaudeTranscript(transcript);
  const exchanges = groupEvents(events);
  const serialized = JSON.stringify({ events, exchanges });
  assert.equal(serialized.includes(prompt), false);
  assert.equal(serialized.includes(response), false);
  assert.equal(exchanges[0].promptPreview, '');
  assert.equal(exchanges[0].tokens.total, 17);
});

test('Codex session detail keeps token metadata but never prompt content', () => {
  const prompt = 'SUPER_SECRET_CODEX_PROMPT';
  const transcript = [
    JSON.stringify({ type: 'event_msg', timestamp: '2026-09-04T00:00:00Z', payload: { type: 'user_message', message: prompt } }),
    JSON.stringify({ type: 'event_msg', timestamp: '2026-09-04T00:00:01Z', payload: { type: 'token_count', info: { last_token_usage: { input_tokens: 20, cached_input_tokens: 5, output_tokens: 6, reasoning_output_tokens: 2 } } } })
  ].join('\n');
  const events = parseCodexTranscript(transcript);
  const exchanges = groupEvents(events);
  const serialized = JSON.stringify({ events, exchanges });
  assert.equal(serialized.includes(prompt), false);
  assert.equal(exchanges[0].promptPreview, '');
  assert.equal(exchanges[0].tokens.total, 26);
  assert.equal(exchanges[0].tokens.reasoning, 2);
});

test('session detail refuses unsupported clients before transcript resolution', () => {
  const detail = readSessionDetail({ client: 'opencode', sessionId: 'anything' });
  assert.equal(detail.found, false);
  assert.deepEqual(detail.exchanges, []);
});

test('preload does not expose privileged mutation/network IPC for disabled surfaces', () => {
  const preload = readRepo('src/electron/preload.js');
  const forbidden = [
    "ipcRenderer.invoke('antigravity:addAccount'",
    "ipcRenderer.invoke('codex:addAccount'",
    "ipcRenderer.invoke('codex:switchSystemAccount'",
    "ipcRenderer.invoke('claude:saveCookie'",
    "ipcRenderer.invoke('cursor:loginManual'",
    "ipcRenderer.invoke('opencode:saveCookie'",
    "ipcRenderer.invoke('openrouter:saveProfile'",
    "ipcRenderer.invoke('thirdparty:saveProfile'",
    "ipcRenderer.invoke('copilot:signIn'",
    "ipcRenderer.invoke('tokscale:downloadFromNpm'",
    "ipcRenderer.invoke('appUpdate:checkNow'",
    "ipcRenderer.invoke('serviceStatus:get'",
    "ipcRenderer.invoke('codexResetForecast:get'"
  ];
  for (const needle of forbidden) assert.equal(preload.includes(needle), false, needle);
  assert.match(preload, /ALLOWED_CLIENTS = new Set\(\['claude', 'codex', 'antigravity'\]\)/);
});

test('Electron and runtime identity hardening remain materialized', () => {
  const main = readRepo('src/electron/main.js');
  assert.match(main, /const APP_NAME = 'Token Lens';/);
  assert.match(main, /app\.setAppUserModelId\('com\.teeeeooo\.tokenlens'\)/);
  assert.match(main, /contextIsolation:\s*true/);
  assert.match(main, /nodeIntegration:\s*false/);
  assert.match(main, /"connect-src 'self'"/);
  assert.match(main, /if \(!DOWNSTREAM_POLICY\.appUpdateChecks\) return;/);
  assert.match(main, /settings = enforceDownstreamSettings\(readSettings\(\)\)/);
});

test('Windows packaging has a distinct unsigned Token Lens identity', () => {
  const config = require('../scripts/downstream/electron-builder.config');
  assert.equal(config.appId, 'com.teeeeooo.tokenlens');
  assert.equal(config.productName, 'Token Lens');
  assert.equal(config.publish, null);
  assert.equal(config.win.signtoolOptions, undefined);
  assert.equal(config.win.verifyUpdateCodeSignature, undefined);
});
