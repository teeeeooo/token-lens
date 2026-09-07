import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';

const main = fs.readFileSync(new URL('../src/main.js', import.meta.url), 'utf8');
const baseCss = fs.readFileSync(new URL('../src/electron/renderer/styles.css', import.meta.url), 'utf8');
const v2Css = fs.readFileSync(new URL('../src/styles.css', import.meta.url), 'utf8');
const tauriConfig = JSON.parse(fs.readFileSync(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8'));

test('v2 uses the fixed system UI font without restoring font settings UI', () => {
  assert.match(baseCss, /--ui-font:\s*system-ui,[^;]*"Segoe UI",\s*sans-serif;/);
  assert.doesNotMatch(main, /interfaceFontFamily|displayFontFamily|fontPreset/i);
});

test('settings remains the primary footer utility instead of the retired swapped layout', () => {
  assert.match(main, /class="utility-actions">/);
  assert.doesNotMatch(main, /class="utility-actions is-swapped"/);
});

test('period tabs stay truly centered and clipped away from window controls', () => {
  assert.match(main, /class="period-controls"/);
  assert.match(main, /class="window-control-region"/);
  assert.doesNotMatch(main, /class="actions-hotspot"/);
  assert.match(v2Css, /grid-template-columns:\s*minmax\(0, 1fr\) 126px minmax\(0, 1fr\)/);
  assert.match(v2Css, /\.period-controls \.tabs \{[^}]*overflow:\s*hidden;/s);
  assert.match(v2Css, /\.period-controls \.tab-indicator \{[^}]*left:\s*var\(--period-indicator-left\);[^}]*width:\s*var\(--period-indicator-width\);[^}]*transform:\s*none;/s);
  assert.match(main, /activeTab\.offsetLeft \+ inset/);
  assert.match(main, /activeTab\.offsetWidth - inset \* 2/);
  assert.match(main, /window\.addEventListener\('resize', syncPeriodIndicator\)/);
  assert.match(v2Css, /\.window-control-region:hover \.window-actions/);
});

test('fresh expanded window is large enough for the complete home surface', () => {
  const mainWindow = tauriConfig.app.windows.find((window) => window.label === 'main');
  assert.equal(mainWindow.width, 380);
  assert.equal(mainWindow.height, 720);
  assert.equal(mainWindow.minWidth, 300);
  assert.equal(mainWindow.maxWidth, 1200);
  assert.equal(mainWindow.maxHeight, 1400);
});
