'use strict';

const { ALLOWED_CLIENTS, filterClientsCsv } = require('./downstreamPolicy');

// Keep upstream client implementations in the tree for low-conflict upstream
// patching, but Token Lens only registers the focused local clients below.
const PARSE_LOCAL_CLIENTS = Object.freeze([]);
const DEFAULT_CLIENTS = ALLOWED_CLIENTS.join(',');

// Preserve the upstream display vocabulary so renderer code and future upstream
// patches remain structurally compatible. This is NOT an execution allowlist;
// clientsCsvForSetting below is the fail-closed runtime boundary.
const UPSTREAM_KNOWN_CLIENTS = Object.freeze([
  'claude', 'codex', 'opencode', 'hermes', 'openclaw', 'cursor', 'antigravity',
  'cline', 'kimi', 'qwen', 'grok', 'copilot', 'pi', 'zed', 'kilocode',
  'commandcode', 'micode', 'zcode', 'kiro', 'codebuddy', 'workbuddy', 'proma',
  'qodercn', 'reasonix', 'dsh', 'cherrystudio', 'lmstudio'
]);
const KNOWN_CLIENTS = UPSTREAM_KNOWN_CLIENTS.join(',');

function normalizeClientsCsv(value) {
  return String(value ?? '')
    .split(',')
    .map((client) => client.trim().toLowerCase())
    .filter(Boolean)
    .join(',');
}

function clientsCsvForSetting(value, fallback = DEFAULT_CLIENTS) {
  const selected = value === undefined || value === null ? fallback : value;
  return filterClientsCsv(selected);
}

module.exports = {
  DEFAULT_CLIENTS,
  PARSE_LOCAL_CLIENTS,
  KNOWN_CLIENTS,
  clientsCsvForSetting,
  normalizeClientsCsv
};
