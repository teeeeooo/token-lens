import assert from 'node:assert/strict';
import test from 'node:test';
import { installTokenMonitorFacade } from '../src/token-monitor-facade.js';

test('installs only the retained compatibility methods implemented so far', () => {
  const target = {};
  const facade = installTokenMonitorFacade(target);

  assert.equal(target.tokenMonitor, facade);
  assert.deepEqual(Object.keys(facade), [
    'getStats',
    'getTokscaleStatus',
    'getSettings',
    'updateSettings',
    'getFloatingBubbleState',
    'collapseFloatingBubbleIfIdle',
    'expandFloatingBubble',
    'peekFloatingBubble',
    'moveFloatingBubble',
  ]);
  assert.equal(typeof facade.getStats, 'function');
  assert.equal(typeof facade.getTokscaleStatus, 'function');
  assert.equal(typeof facade.getSettings, 'function');
  assert.equal(typeof facade.expandFloatingBubble, 'function');
  assert.equal(typeof facade.moveFloatingBubble, 'function');
  assert.equal(Object.isFrozen(facade), true);
});

test('facade binding cannot be overwritten', () => {
  const target = {};
  installTokenMonitorFacade(target);
  assert.throws(() => {
    target.tokenMonitor = {};
  }, TypeError);
});
