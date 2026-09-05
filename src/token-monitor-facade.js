import {
  collapseFloatingBubbleIfIdle,
  expandFloatingBubble,
  getFloatingBubbleState,
  getSettings,
  getTokscaleStatus,
  moveFloatingBubble,
  peekFloatingBubble,
  updateSettings,
  updateTraySummary,
} from './backend.js';
import { getStats } from './stats-compat.js';

export function installTokenMonitorFacade(target = window) {
  const facade = Object.freeze({
    getStats,
    getTokscaleStatus,
    getSettings,
    updateSettings,
    updateTraySummary,
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
