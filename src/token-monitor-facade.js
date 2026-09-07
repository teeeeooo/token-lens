import {
  collapseFloatingBubbleIfIdle,
  expandFloatingBubble,
  getDashboardHistory,
  getFloatingBubbleState,
  getSessionDetail,
  getSettings,
  getTokscaleStatus,
  minimizeMainWindow,
  moveFloatingBubble,
  openProviderErrorLogDirectory,
  peekFloatingBubble,
  setFloatingBubbleWidth,
  updateSettings,
  updateTraySummary,
} from './backend.js';
import { getStats } from './stats-compat.js';

export function installTokenMonitorFacade(target = window) {
  const facade = Object.freeze({
    getStats,
    getDashboardHistory,
    getSessionDetail,
    getTokscaleStatus,
    getSettings,
    updateSettings,
    openProviderErrorLogDirectory,
    updateTraySummary,
    getFloatingBubbleState,
    collapseFloatingBubbleIfIdle,
    minimizeMainWindow,
    setFloatingBubbleWidth,
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
