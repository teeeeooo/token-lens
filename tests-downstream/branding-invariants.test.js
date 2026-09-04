'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');

const repoRoot = path.join(__dirname, '..');
const readRepo = (relativePath) => fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');

test('user-facing product branding is Token Lens while upstream identifiers stay intact', () => {
  const surfaces = [
    'src/electron/renderer/index.html',
    'src/electron/renderer/i18n.js',
    'src/electron/tray.js'
  ];

  for (const relativePath of surfaces) {
    const source = readRepo(relativePath);
    assert.equal(source.includes('Token Monitor'), false, `${relativePath} still exposes the upstream product name`);
  }

  const html = readRepo('src/electron/renderer/index.html');
  const i18n = readRepo('src/electron/renderer/i18n.js');
  const tray = readRepo('src/electron/tray.js');

  assert.match(html, /<title>Token Lens<\/title>/);
  assert.match(html, /Show Token Lens/);
  assert.match(i18n, /'settings\.about\.title': 'About Token Lens'/);
  assert.match(i18n, /'settings\.display\.trayProviderBadge': 'Show Token Lens badge on provider icons'/);
  assert.match(i18n, /'trayMenu\.quit': 'Quit Token Lens'/);
  assert.match(tray, /tray\.setToolTip\('Token Lens'\)/);

  // Do not rename upstream-compatible JS API symbols; only the spaced product name is branded.
  assert.match(i18n, /TokenMonitorI18n/);
});
