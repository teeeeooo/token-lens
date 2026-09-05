import assert from 'node:assert/strict';
import test from 'node:test';
import {
  THEME_PRESETS,
  applyThemePreset,
  normalizeThemePreset,
  normalizeZoomFactor,
  themeCssEntries,
} from '../src/appearance-model.js';

test('retained appearance presets are narrow and stable', () => {
  assert.deepEqual(THEME_PRESETS.map((preset) => preset.id), ['default', 'obsidian', 'porcelain']);
  assert.equal(normalizeThemePreset('PORCELAIN'), 'porcelain');
  assert.equal(normalizeThemePreset('custom'), 'default');
});

test('porcelain flips the inherited surface system to light mode', () => {
  const entries = Object.fromEntries(themeCssEntries('porcelain').map(({ name, value }) => [name, value]));
  assert.equal(entries['color-scheme'], 'light');
  assert.equal(entries['--glass-rgb'], '246, 247, 249');
  assert.equal(entries['--accent'], '#2563eb');
  assert.equal(entries['--number'], '#1c1f26');
});
test('theme application updates only the preset-owned CSS variables', () => {
  const values = new Map();
  const root = {
    dataset: {},
    style: { setProperty(name, value) { values.set(name, value); } },
  };
  assert.equal(applyThemePreset(root, 'obsidian'), 'obsidian');
  assert.equal(root.dataset.themePreset, 'obsidian');
  assert.equal(values.get('--accent'), '#e6e8ec');
  assert.equal(values.get('color-scheme'), 'dark');
});

test('zoom follows the retained 70 to 160 percent range in 10 percent steps', () => {
  assert.equal(normalizeZoomFactor(0.1), 0.7);
  assert.equal(normalizeZoomFactor(1.24), 1.2);
  assert.equal(normalizeZoomFactor(1.26), 1.3);
  assert.equal(normalizeZoomFactor(9), 1.6);
  assert.equal(normalizeZoomFactor('nope'), 1);
});
