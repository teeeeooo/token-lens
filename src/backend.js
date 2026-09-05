import { invoke } from '@tauri-apps/api/core';

export function getUsageReport(period = 'today', grouping = 'client_model') {
  return invoke('get_usage_report', { period, grouping });
}

export function getUsageSinceReport(since, grouping = 'client_model') {
  return invoke('get_usage_since_report', { since, grouping });
}

export function getQuotaReport() {
  return invoke('get_quota_report');
}

export function getTokscaleStatus() {
  return invoke('get_tokscale_status');
}
