import test from 'node:test';
import assert from 'node:assert/strict';
import { normalizeLanguage, resolveLanguage, translate } from '../src/i18n.js';

test('language defaults to system auto and resolves Korean Windows locale', () => {
  assert.equal(normalizeLanguage(''), 'auto');
  assert.equal(normalizeLanguage('xx-test'), 'auto');
  assert.equal(resolveLanguage('auto', ['ko-KR', 'en-US']), 'ko');
  assert.equal(translate('ko', 'settings.language.auto'), '자동 (시스템 설정)');
});

test('explicit language overrides system language and retained locales stay translated', () => {
  assert.equal(resolveLanguage('en', ['ko-KR']), 'en');
  assert.equal(translate('ja', 'settings.trayIcon'), 'トレイアイコン');
});
