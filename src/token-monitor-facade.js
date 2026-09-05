import {
  collapseFloatingBubbleIfIdle,
  expandFloatingBubble,
  getFloatingBubbleState,
  getSettings,
  getTokscaleStatus,
  moveFloatingBubble,
  peekFloatingBubble,
  updateSettings,
} from './backend.js';
import { getStats } from './stats-compat.js';

export function installTokenMonitorFacade(target = window) {
  const facade = Object.freeze({
    getStats,
    getTokscaleStatus,
    getSettings,
    updateSettings,
    getFloatingBubbleState,
    collapseFloatingBubbleIfIdle,
    expandFloatingBubble,
    peekFloatingBubble,
    moveFloatingBubble,
  });

  Object.defineProperty(target, 'tokenMonitor', {
    configurable: false,
    enumerable: true,
    writable: false,
    value: facade,
  });

  return facade;
}
