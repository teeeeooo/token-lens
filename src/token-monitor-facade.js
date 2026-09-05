import { getTokscaleStatus } from './backend.js';
import { getStats } from './stats-compat.js';

export function installTokenMonitorFacade(target = window) {
  const facade = Object.freeze({
    getStats,
    getTokscaleStatus,
  });

  Object.defineProperty(target, 'tokenMonitor', {
    configurable: false,
    enumerable: true,
    writable: false,
    value: facade,
  });

  return facade;
}
