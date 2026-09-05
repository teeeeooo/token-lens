import { invoke } from '@tauri-apps/api/core';

export function getUsageReport(period = 'today', grouping = 'client_model') {
  return invoke('get_usage_report', { period, grouping });
}

export function getUsageSinceReport(since, grouping = 'client_model') {
  return invoke('get_usage_since_report', { since, grouping });
}

export function getSessionDetail({ client, sessionId, startTimeMs = null, sessionCost = 0 } = {}) {
  return invoke('get_session_detail', { client, sessionId, startTimeMs, sessionCost });
}

export function getQuotaReport() {
  return invoke('get_quota_report');
}

export function getTokscaleStatus() {
  return invoke('get_tokscale_status');
}

export function getSessionMetadata(sessions = []) {
  return invoke('get_session_metadata', { sessions });
}

export function getSettings() {
  return invoke('get_settings');
}

export function updateSettings(patch) {
  return invoke('update_settings', { patch });
}

export function getFloatingBubbleState() {
  return invoke('get_floating_bubble_state');
}

export function collapseFloatingBubbleIfIdle() {
  return invoke('collapse_floating_bubble_if_idle');
}
export function expandFloatingBubble() {
  return invoke('expand_floating_bubble');
}

export function peekFloatingBubble() {
  return invoke('peek_floating_bubble');
}

export function moveFloatingBubble(offset = {}) {
  return invoke('move_floating_bubble', { offset });
}

export function updateTraySummary(summary) {
  return invoke('update_tray_summary', { summary });
}
