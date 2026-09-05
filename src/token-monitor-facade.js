import { getTokscaleStatus } from './backend.js';

export function installTokenMonitorFacade(target = window) {
  const facade = Object.freeze({
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
