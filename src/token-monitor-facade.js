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
import {
  getBootstrapStats,
  getPeriodStats,
  getQuotaLimits,
  getStats,
  preloadSlowUsage,
} from './stats-compat.js';

export function installTokenMonitorFacade(target = window) {
  const facade = Object.freeze({
    getStats,
    getBootstrapStats,
    getPeriodStats,
    getQuotaLimits,
    preloadSlowUsage,
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
