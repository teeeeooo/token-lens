import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';

const main = fs.readFileSync(new URL('../src/main.js', import.meta.url), 'utf8');
const baseCss = fs.readFileSync(new URL('../src/electron/renderer/styles.css', import.meta.url), 'utf8');
const v2Css = fs.readFileSync(new URL('../src/styles.css', import.meta.url), 'utf8');

test('v2 uses the fixed system UI font without restoring font settings UI', () => {
  assert.match(baseCss, /--ui-font:\s*system-ui,[^;]*"Segoe UI",\s*sans-serif;/);
  assert.doesNotMatch(main, /interfaceFontFamily|displayFontFamily|fontPreset/i);
});

test('settings remains the primary footer utility instead of the retired swapped layout', () => {
  assert.match(main, /class="utility-actions">/);
  assert.doesNotMatch(main, /class="utility-actions is-swapped"/);
});

test('period tabs and window actions use separate non-overlapping titlebar regions', () => {
  assert.match(main, /class="period-controls"/);
  assert.match(main, /class="window-control-region"/);
  assert.doesNotMatch(main, /class="actions-hotspot"/);
  assert.match(v2Css, /grid-template-columns:\s*minmax\(0, 1fr\) 126px 92px/);
  assert.match(v2Css, /\.window-control-region:hover \.window-actions/);
});
