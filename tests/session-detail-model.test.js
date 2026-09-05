import assert from 'node:assert/strict';
import test from 'node:test';
import { exchangeRows, formatToolList, periodStartTimeMs } from '../src/session-detail-model.js';

test('session detail rows expose only neutral exchange labels and usage metadata', () => {
  const detail = {
    exchanges: [{
      startedAt: '2026-09-05T01:00:00Z', turnCount: 1, tools: ['shell', 'shell'],
      tokens: { total: 26 }, costEstimate: 0.5,
      turns: [{
        timestamp: '2026-09-05T01:00:00Z',
        tokens: { input: 15, output: 6, cacheRead: 5, cacheWrite: 0, reasoning: 2, total: 26 },
        tools: ['shell'], costEstimate: 0.5,
      }],
    }],
  };
  const rows = exchangeRows(detail, { now: new Date('2026-09-05T03:00:00Z') });
  assert.equal(rows[0].title, 'Exchange #1');
  assert.equal(rows[0].value, 26);
  assert.equal(rows[0].turns[0].label, 'Reply #1');
  assert.equal(rows[0].turns[0].tokens.reasoning, 2);
  assert.equal(formatToolList(['shell', 'shell', 'Read']), 'shell · Read');
  assert.equal(JSON.stringify(rows).includes('prompt'), false);
});

test('session detail period starts use local calendar boundaries', () => {
  const now = new Date(2026, 8, 5, 14, 30, 0);
  assert.equal(periodStartTimeMs('today', { now }), new Date(2026, 8, 5).getTime());
  assert.equal(periodStartTimeMs('month', { now }), new Date(2026, 8, 1).getTime());
  assert.equal(periodStartTimeMs('last7', { now }), new Date(2026, 7, 30).getTime());
  assert.equal(periodStartTimeMs('last30', { now }), new Date(2026, 7, 7).getTime());
  assert.equal(periodStartTimeMs('allTime', { now }), null);
});
