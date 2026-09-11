import assert from 'node:assert/strict';
import test from 'node:test';
import {
  bubblePercentLabel,
  configuredBubbleSelections,
  floatingBubbleModel,
  normalizeBubbleContent,
  normalizeBubbleProviders,
  normalizeBubbleScale,
} from '../src/floating-bubble-model.js';

function limits(providers) {
  return { providers };
}

function provider(provider, windows) {
  return {
    provider,
    status: 'ok',
    planLabel: 'Test',
    windows: windows.map((window) => ({ showMeter: true, ...window })),
  };
}

test('bubble scale stays within the persisted 70 to 150 percent range', () => {
  assert.equal(normalizeBubbleScale(0.2), 0.7);
  assert.equal(normalizeBubbleScale(1.24), 1.2);
  assert.equal(normalizeBubbleScale(2), 1.5);
  assert.equal(normalizeBubbleScale('bad'), 1);
});

test('provider limits use fixed v2 provider order and primary quota semantics', () => {
  const value = limits([
    provider('claude', [
      { kind: 'session', label: '5h', remainingPercent: 41 },
      { kind: 'weekly', label: 'Weekly', remainingPercent: 72 },
    ]),
    provider('codex', [
      { kind: 'session', label: '5h', remainingPercent: 62 },
      { kind: 'weekly', label: 'Weekly', remainingPercent: 84 },
    ]),
    provider('gemini', [{ kind: 'other', label: 'Gemini 2.5', remainingPercent: 18 }]),
  ]);
  const picks = configuredBubbleSelections(value);
  assert.deepEqual(picks.map((pick) => pick.providerId), ['codex', 'claude']);
  assert.equal(picks[0].primaryPercent, 62);
  assert.equal(picks[0].secondaryPercent, 84);

  const model = floatingBubbleModel(value, 'limitsAllSessions');
  assert.equal(model.kind, 'limits');
  assert.deepEqual(model.entries.map((entry) => [entry.providerId, entry.percents]), [
    ['codex', [62]],
    ['claude', [41]],
  ]);
});


test('provider limits can show one through four explicitly selected providers in fixed order', () => {
  const value = limits([
    provider('antigravity', [{ kind: 'session', label: '5h', remainingPercent: 44 }]),
    provider('gemini', [{ kind: 'other', label: 'Gemini Pro', remainingPercent: 55 }]),
    provider('claude', [{ kind: 'session', label: '5h', remainingPercent: 66 }]),
    provider('codex', [{ kind: 'session', label: '5h', remainingPercent: 77 }]),
  ]);
  const four = floatingBubbleModel(value, 'limitsAllSessions', ['antigravity', 'gemini', 'claude', 'codex']);
  assert.equal(four.kind, 'limits');
  assert.deepEqual(four.entries.map((entry) => entry.providerId), ['codex', 'claude', 'gemini', 'antigravity']);
  const three = floatingBubbleModel(value, 'limitsAllSessions', ['antigravity', 'gemini', 'claude']);
  assert.deepEqual(three.entries.map((entry) => entry.providerId), ['claude', 'gemini', 'antigravity']);
  const one = floatingBubbleModel(value, 'limitsAllSessions', ['gemini']);
  assert.deepEqual(one.entries.map((entry) => entry.providerId), ['gemini']);
});

test('provider selection applies only to provider limits and unavailable selections are not substituted', () => {
  const value = limits([
    provider('codex', [{ kind: 'session', label: '5h', remainingPercent: 66 }]),
    provider('claude', [{ kind: 'session', label: '5h', remainingPercent: 33 }]),
    provider('gemini', [{ kind: 'other', label: 'Gemini Pro', remainingPercent: 22 }]),
  ]);
  const selected = floatingBubbleModel(value, 'limitsAllSessions', ['gemini', 'antigravity']);
  assert.deepEqual(selected.entries.map((entry) => entry.providerId), ['gemini']);
  const bars = floatingBubbleModel(value, 'barsAllSessions', ['gemini']);
  assert.equal(bars.kind, 'barsPair');
  assert.deepEqual(bars.percents, [66, 33]);
});

test('bubble provider normalization is allowlisted and fixed-order without a count cap', () => {
  assert.deepEqual(
    normalizeBubbleProviders(['antigravity', 'gemini', 'codex', 'claude', 'gemini', 'invalid']),
    ['codex', 'claude', 'gemini', 'antigravity'],
  );
  assert.deepEqual(normalizeBubbleProviders(null), []);
});

test('single-provider limits show its primary and secondary quotas', () => {
  const model = floatingBubbleModel(limits([
    provider('codex', [
      { kind: 'session', label: '5h', remainingPercent: 55 },
      { kind: 'weekly', label: 'Weekly', remainingPercent: 77 },
    ]),
  ]), 'limitsAllSessions');
  assert.equal(model.kind, 'limits');
  assert.deepEqual(model.entries[0].percents, [55, 77]);
});

test('bar modes retain v1 worst-window selection semantics', () => {
  const value = limits([
    provider('codex', [
      { kind: 'session', label: '5h', remainingPercent: 60 },
      { kind: 'weekly', label: 'Weekly', remainingPercent: 15 },
    ]),
    provider('claude', [
      { kind: 'session', label: '5h', remainingPercent: 20 },
      { kind: 'weekly', label: 'Weekly', remainingPercent: 50 },
    ]),
  ]);
  const session = floatingBubbleModel(value, 'barsSession');
  assert.equal(session.kind, 'providerBars');
  assert.equal(session.providerId, 'claude');

  const weekly = floatingBubbleModel(value, 'barsWeekly');
  assert.equal(weekly.kind, 'providerBars');
  assert.equal(weekly.providerId, 'codex');

  const lowest = floatingBubbleModel(value, 'bars');
  assert.equal(lowest.kind, 'providerBars');
  assert.equal(lowest.providerId, 'codex');
});

test('additional Codex quota lanes are ignored by compact bubble selection', () => {
  const model = floatingBubbleModel(limits([
    provider('codex', [
      { kind: 'session', label: '5h', remainingPercent: 80 },
      { kind: 'weekly', label: 'Reserve', remainingPercent: 1, additional: true },
      { kind: 'weekly', label: 'Weekly', remainingPercent: 70 },
    ]),
  ]), 'bars');
  assert.equal(model.providerId, 'codex');
  assert.equal(model.primaryPercent, 80);
  assert.equal(model.secondaryPercent, 70);
});

test('model quotas with other cadence remain eligible as a fallback', () => {
  const model = floatingBubbleModel(limits([
    provider('gemini', [{ kind: 'other', label: 'gemini-2.5-pro', remainingPercent: 18 }]),
  ]), 'limitsAllSessions');
  assert.equal(model.kind, 'limits');
  assert.equal(model.entries[0].providerId, 'gemini');
  assert.deepEqual(model.entries[0].percents, [18]);
});

test('unknown modes normalize to provider limits and percent labels stay compact', () => {
  assert.equal(normalizeBubbleContent('custom'), 'limitsAllSessions');
  assert.equal(normalizeBubbleContent('icon'), 'icon');
  assert.equal(bubblePercentLabel(62.4), '62%');
  assert.equal(bubblePercentLabel(101), '100%');
  assert.equal(bubblePercentLabel(null), '');
});


test('first-two-provider bars preserve order without adding provider glyph rows', () => {
  const value = limits([
    provider('claude', [{ kind: 'session', label: '5h', remainingPercent: 33 }]),
    provider('codex', [{ kind: 'session', label: '5h', remainingPercent: 66 }]),
  ]);
  const pair = floatingBubbleModel(value, 'barsAllSessions');
  assert.equal(pair.kind, 'barsPair');
  assert.deepEqual(pair.percents, [66, 33]);

  const single = floatingBubbleModel(limits([
    provider('codex', [
      { kind: 'session', label: '5h', remainingPercent: 66 },
      { kind: 'weekly', label: 'Weekly', remainingPercent: 44 },
    ]),
  ]), 'barsAllSessions');
  assert.equal(single.kind, 'providerBars');
  assert.equal(single.primaryPercent, 66);
  assert.equal(single.secondaryPercent, 44);
});

test('quota display falls back to the sigma icon when no provider quota is usable', () => {
  const unavailable = { providers: [{ provider: 'codex', status: 'unavailable', windows: [] }] };
  assert.deepEqual(floatingBubbleModel(unavailable, 'limitsAllSessions'), { kind: 'icon' });
  assert.deepEqual(floatingBubbleModel(unavailable, 'bars'), { kind: 'icon' });
});
