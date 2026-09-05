import assert from 'node:assert/strict';
import test from 'node:test';
import {
  derivedRequest,
  displayLabel,
  periodMenuTargetIndex,
  rangeForSelection,
  slotForSelection,
} from '../src/fixed-periods.js';

test('fixed period slots preserve the original month-group presentation', () => {
  assert.equal(slotForSelection('today'), 'today');
  assert.equal(slotForSelection('month'), 'month');
  assert.equal(slotForSelection('week'), 'month');
  assert.equal(slotForSelection('last7'), 'month');
  assert.equal(slotForSelection('last30'), 'month');
  assert.equal(displayLabel('month'), 'MONTH');
  assert.equal(displayLabel('week'), 'WEEK');
  assert.equal(displayLabel('last7'), '7D');
  assert.equal(displayLabel('last30'), '30D');
});

test('derived fixed ranges match the v1 inclusive-day semantics', () => {
  const todayKey = '2026-09-05';
  assert.deepEqual(rangeForSelection('last7', { todayKey }), { start: '2026-08-30', end: todayKey });
  assert.deepEqual(rangeForSelection('last30', { todayKey }), { start: '2026-08-07', end: todayKey });
  assert.deepEqual(derivedRequest('last7', { todayKey }), { key: 'last7', since: '2026-08-30' });
});

test('this-week range honors locale first-day semantics', () => {
  const todayKey = '2026-09-05'; // Saturday
  assert.deepEqual(rangeForSelection('week', { todayKey, locale: 'ko-KR' }), {
    start: '2026-08-30',
    end: todayKey,
  });
  assert.deepEqual(rangeForSelection('week', { todayKey, locale: 'en-GB' }), {
    start: '2026-08-31',
    end: todayKey,
  });
});

test('period menu keyboard navigation wraps and supports endpoints', () => {
  assert.equal(periodMenuTargetIndex('ArrowDown', 3, 4), 0);
  assert.equal(periodMenuTargetIndex('ArrowUp', 0, 4), 3);
  assert.equal(periodMenuTargetIndex('Home', 2, 4), 0);
  assert.equal(periodMenuTargetIndex('End', 1, 4), 3);
  assert.equal(periodMenuTargetIndex('Escape', 1, 4), -1);
});
