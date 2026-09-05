import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

const CONFIG_URL = new URL('../src-tauri/tauri.conf.json', import.meta.url);

function directives(csp) {
  return new Map(csp.split(';').map((part) => {
    const [name, ...values] = part.trim().split(/\s+/);
    return [name, values];
  }).filter(([name]) => name));
}

test('Tauri renderer keeps a restrictive CSP with only style inline relaxation', async () => {
  const config = JSON.parse(await readFile(CONFIG_URL, 'utf8'));
  const csp = config.app?.security?.csp;
  assert.equal(typeof csp, 'string');
  const policy = directives(csp);

  assert.deepEqual(policy.get('default-src'), ["'self'"]);
  assert.deepEqual(policy.get('script-src'), ["'self'"]);
  assert.deepEqual(policy.get('style-src'), ["'self'", "'unsafe-inline'"]);
  assert.deepEqual(policy.get('connect-src'), ["'self'", 'ipc:', 'http://ipc.localhost']);
  assert.deepEqual(policy.get('object-src'), ["'none'"]);
  assert.deepEqual(policy.get('base-uri'), ["'none'"]);
  assert.deepEqual(policy.get('form-action'), ["'none'"]);
  assert.deepEqual(policy.get('frame-ancestors'), ["'none'"]);
  assert.ok(!policy.get('script-src').includes("'unsafe-inline'"));
});