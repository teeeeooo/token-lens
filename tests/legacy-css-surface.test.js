import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

const CSS_URL = new URL('../src/electron/renderer/styles.css', import.meta.url);

const RETIRED_MARKERS = [
  'session-archive',
  'settings-export-block',
  'export-',
  'diagnostic',
  'hub-',
  'service-status',
  'app-update',
  'update-pill',
  'limit-account-switch',
];

test('retired v1 subsystem CSS is absent while provider extension assets remain', async () => {
  const css = await readFile(CSS_URL, 'utf8');
  for (const marker of RETIRED_MARKERS) {
    assert.ok(!css.includes(marker), `retired CSS marker remained: ${marker}`);
  }
  for (const provider of ['cursor', 'opencode', 'qoder', 'ollama']) {
    assert.ok(css.includes(`.row-icon-${provider}`), `provider extension style missing: ${provider}`);
  }
});