'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');

const {
  ALLOWED_CLIENTS,
  ALLOWED_LIMIT_PROVIDERS,
  DOWNSTREAM_POLICY,
  enforceDownstreamSettings,
  filterClientsCsv,
  filterLimitProviders,
  scrubSessionDetail
} = require('../../src/shared/downstreamPolicy');
const { clientsCsvForSetting } = require('../../src/shared/clientTracking');
const { LIMIT_PROVIDER_IDS } = require('../../src/shared/limitProviders');

const root = path.join(__dirname, '..', '..');

function read(...parts) {
  return fs.readFileSync(path.join(root, ...parts), 'utf8');
}

test('focused provider policy is exactly Claude, Codex and Antigravity', () => {
  assert.deepEqual(ALLOWED_CLIENTS, ['claude', 'codex', 'antigravity']);
  assert.deepEqual(ALLOWED_LIMIT_PROVIDERS, ['claude', 'codex', 'antigravity']);
  assert.deepEqual([...LIMIT_PROVIDER_IDS], ['claude', 'codex', 'antigravity']);
});

test('usage client settings fail closed to the focused allowlist', () => {
  assert.equal(filterClientsCsv('cursor,codex,CLAUDE,antigravity,codex'), 'codex,claude,antigravity');
  assert.equal(clientsCsvForSetting('deepseek,cursor'), '');
  assert.equal(clientsCsvForSetting(undefined), 'claude,codex,antigravity');
});

test('limit provider settings fail closed to the focused allowlist', () => {
  assert.equal(filterLimitProviders('openrouter,antigravity,codex,claude'), 'antigravity,codex,claude');
});

test('persisted or environment-derived settings cannot re-enable blocked capabilities', () => {
  const hardened = enforceDownstreamSettings({
    hubMode: 'host',
    hubUrl: 'http://example.invalid:17321',
    hubHostSecret: 'secret',
    secret: 'secret',
    discordRpcEnabled: true,
    automaticAppUpdates: true,
    claudeWebCookie: 'sessionKey=secret',
    clients: 'cursor,codex',
    limitProviders: 'thirdparty,codex'
  });

  assert.equal(hardened.hubMode, 'local');
  assert.equal(hardened.hubUrl, '');
  assert.equal(hardened.hubHostSecret, '');
  assert.equal(hardened.secret, '');
  assert.equal(hardened.discordRpcEnabled, false);
  assert.equal(hardened.automaticAppUpdates, false);
  assert.equal(hardened.claudeWebCookie, '');
  assert.equal(hardened.clients, 'codex');
  assert.equal(hardened.limitProviders, 'codex');
  assert.equal(DOWNSTREAM_POLICY.runtimeDownloads, false);
  assert.equal(DOWNSTREAM_POLICY.appUpdateChecks, false);
  assert.equal(DOWNSTREAM_POLICY.appUpdates, false);
});

test('session detail retains usage metadata but removes prompt previews', () => {
  const result = scrubSessionDetail({
    found: true,
    totals: { totalTokens: 123 },
    exchanges: [{
      promptPreview: 'internal project prompt',
      startedAt: '2026-09-04T00:00:00.000Z',
      turnCount: 2,
      tokens: { total: 123 },
      tools: ['read', 'bash']
    }]
  });

  assert.equal(result.totals.totalTokens, 123);
  assert.equal(result.exchanges[0].promptPreview, '');
  assert.equal(result.exchanges[0].turnCount, 2);
  assert.deepEqual(result.exchanges[0].tools, ['read', 'bash']);
});

test('main process contains downstream fail-closed gates', () => {
  const main = read('src', 'electron', 'main.js');
  assert.match(main, /DOWNSTREAM_POLICY, enforceDownstreamSettings/);
  assert.match(main, /settings = enforceDownstreamSettings\(readSettings\(\)\)/);
  assert.match(main, /settings = enforceDownstreamSettings\(settings\)/);
  assert.match(main, /if \(!DOWNSTREAM_POLICY\.appUpdateChecks\) return;/);
  assert.match(main, /DOWNSTREAM_POLICY\.runtimeDownloads/);
  assert.match(main, /DOWNSTREAM_POLICY\.appUpdates/);
  assert.match(main, /if \(!DOWNSTREAM_POLICY\.hub\)/);
  assert.match(main, /if \(!DOWNSTREAM_POLICY\.claudeWebCookie\)/);
});

test('upstream Electron renderer isolation controls remain intact', () => {
  const main = read('src', 'electron', 'main.js');
  assert.match(main, /contextIsolation:\s*true/);
  assert.match(main, /nodeIntegration:\s*false/);
  assert.match(main, /"connect-src 'self'"/);
  assert.match(main, /"object-src 'none'"/);
});
