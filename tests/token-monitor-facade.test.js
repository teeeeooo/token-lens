import assert from 'node:assert/strict';
import test from 'node:test';
import { installTokenMonitorFacade } from '../src/token-monitor-facade.js';

test('installs only the first retained compatibility method', () => {
  const target = {};
  const facade = installTokenMonitorFacade(target);

  assert.equal(target.tokenMonitor, facade);
  assert.deepEqual(Object.keys(facade), ['getTokscaleStatus']);
  assert.equal(typeof facade.getTokscaleStatus, 'function');
  assert.equal(Object.isFrozen(facade), true);
});

test('facade binding cannot be overwritten', () => {
  const target = {};
  installTokenMonitorFacade(target);
  assert.throws(() => {
    target.tokenMonitor = {};
  }, TypeError);
});
