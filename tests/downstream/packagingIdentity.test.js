'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');

const root = path.join(__dirname, '..', '..');

test('runtime state is isolated from upstream Token Monitor', () => {
  const main = fs.readFileSync(path.join(root, 'src', 'electron', 'main.js'), 'utf8');
  assert.match(main, /app\.setName\('Token Lens'\)/);
  assert.match(main, /app\.setPath\('userData', tokenLensUserData\)/);
  assert.match(main, /process\.env\.TOKEN_MONITOR_SHARED_DIR = tokenLensUserData/);
});

test('Windows packages use a distinct unsigned downstream application identity', () => {
  const config = require('../../scripts/downstream/electron-builder.config');
  assert.equal(config.appId, 'com.teeeeooo.tokenlens');
  assert.equal(config.productName, 'Token Lens');
  assert.equal(config.publish, null);
  assert.equal(Object.prototype.hasOwnProperty.call(config.win, 'signtoolOptions'), false);
  assert.equal(Object.prototype.hasOwnProperty.call(config.win, 'verifyUpdateCodeSignature'), false);
  assert.equal(config.nsis.artifactName, 'Token-Lens-Setup-${version}.${ext}');
  assert.equal(config.portable.artifactName, 'Token-Lens-${version}.${ext}');
});
