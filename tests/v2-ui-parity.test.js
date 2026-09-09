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

test('bubble size is a persisted 70 to 150 percent control with scaled bubble CSS', () => {
  assert.match(main, /id="floatingBubbleScaleInput" type="range" min="70" max="150" step="10"/);
  assert.match(main, /floatingBubbleScale:\s*normalizeBubbleScale/);
  assert.match(main, /--bubble-scale/);
  assert.match(v2Css, /calc\(34px \* var\(--bubble-scale, 1\)\)/);
});

test('bubble width sync cannot replay stale collapsed state after expansion', () => {
  const match = main.match(/async function syncFloatingBubbleWidth\(\) \{([\s\S]*?)\n\}/);
  assert.ok(match);
  assert.match(match[1], /setFloatingBubbleWidth/);
  assert.doesNotMatch(match[1], /applyFloatingBubbleState/);
  assert.match(main, /applyFloatingBubbleState\(next\);/);
});

test('home quota balances use full credit labels and one-decimal money formatting', () => {
  assert.match(main, /t\('quota\.credits', \{ value: count \}\)/);
  assert.doesNotMatch(main, /\$\{count\} cr/);
  assert.match(main, /maximumFractionDigits:\s*1/);
  assert.match(main, /home-limit-window-wide/);
});

test('home quota warning color is provider-neutral yellow', () => {
  assert.match(baseCss, /\.home-limit-value-low\s*\{[^}]*color:\s*var\(--yellow\);/s);
  assert.doesNotMatch(main, /--home-limit-accent/);
});

test('fresh expanded window is large enough for the complete home surface', () => {
  const mainWindow = tauriConfig.app.windows.find((window) => window.label === 'main');
  assert.equal(mainWindow.width, 380);
  assert.equal(mainWindow.height, 720);
  assert.equal(mainWindow.minWidth, 300);
  assert.equal(mainWindow.maxWidth, 1200);
  assert.equal(mainWindow.maxHeight, 1400);
});


test('provider filter and Gemini quota controls remain bounded at narrow window widths', () => {
  assert.match(v2Css, /\.provider-filter-menu\s*\{[^}]*max-width:\s*calc\(100vw - 28px\);/s);
  assert.match(v2Css, /\.limit-windows-gemini\s*\{[^}]*grid-template-columns:\s*minmax\(0, 1fr\);/s);
  assert.match(v2Css, /@media \(max-width:\s*340px\)[\s\S]*?\.limit-window-text-selectable\s*\{[^}]*display:\s*grid;/s);
  assert.match(v2Css, /\.breakdown \.row-name\s*\{[^}]*overflow:\s*hidden;/s);
});
