import assert from 'node:assert/strict';
import test from 'node:test';
import {
  heatIntensityRows,
  historySinceKey,
  historyViewModel,
  localDayKey,
  patchToday,
  rollingHeatmap,
  trendModel,
} from '../src/history-model.js';

test('history range uses local calendar days', () => {
  const now = new Date(2026, 8, 5, 23, 30, 0);
  assert.equal(localDayKey(now), '2026-09-05');
  assert.equal(historySinceKey(now, 7), '2026-08-30');
});

test('today patch replaces stale history without mutating the source', () => {
  const source = [{ date: '2026-09-05', tokens: 10, cost: 0.1 }];
  const patched = patchToday(source, { totalTokens: 25, costUsd: 0.4 }, new Date(2026, 8, 5));
  assert.equal(source[0].tokens, 10);
  assert.equal(patched[0].tokens, 25);
  assert.equal(patched[0].cost, 0.4);
});

test('heat intensity and rolling grid preserve sparse days', () => {
  const rows = heatIntensityRows([
    { date: '2026-09-01', tokens: 100 },
    { date: '2026-09-03', tokens: 400 },
  ]);
  assert.equal(rows[0].intensity, 2);
  assert.equal(rows[1].intensity, 4);
  const grid = rollingHeatmap(rows, { endDate: '2026-09-05', days: 7 });
  assert.ok(grid.cells.some((cell) => cell.date === '2026-09-02' && cell.tokens === 0));
  assert.ok(grid.cells.some((cell) => cell.date === '2026-09-03' && cell.intensity === 4));
});

test('trend and combined view expose retained activity summary', () => {
  const daily = [
    { date: '2026-09-03', tokens: 20 },
    { date: '2026-09-04', tokens: 40 },
    { date: '2026-09-05', tokens: 10 },
  ];
  const trend = trendModel(daily, { width: 100, height: 40 });
  assert.equal(trend.dates[1], '2026-09-04');
  assert.match(trend.line, /^M/);
  const view = historyViewModel({ daily, summary: { activeDays: 3 } }, { totalTokens: 50 }, new Date(2026, 8, 5));
  assert.equal(view.activeDays, 3);
  assert.equal(view.peakDayTokens, 50);
  assert.equal(view.daily.at(-1).tokens, 50);
});
