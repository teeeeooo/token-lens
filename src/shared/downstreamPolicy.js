'use strict';

// Token Lens is a private, security-hardened downstream distribution of
// Javis603/token-monitor. Keep policy in one small module so upstream provider
// fixes can be absorbed without scattering downstream-only conditionals.
const ALLOWED_CLIENTS = Object.freeze(['claude', 'codex', 'antigravity']);
const ALLOWED_LIMIT_PROVIDERS = Object.freeze(['claude', 'codex', 'antigravity']);
const ALLOWED_CLIENT_SET = new Set(ALLOWED_CLIENTS);
const ALLOWED_LIMIT_PROVIDER_SET = new Set(ALLOWED_LIMIT_PROVIDERS);

const DOWNSTREAM_POLICY = Object.freeze({
  hub: false,
  remoteSync: false,
  embeddedHttpServer: false,
  discordRpc: false,
  runtimeDownloads: false,
  appUpdateChecks: false,
  appUpdates: false,
  claudeWebCookie: false,
  promptPreview: false
});

function normalizedIds(value) {
  const values = Array.isArray(value) ? value : String(value ?? '').split(',');
  return values
    .map((id) => String(id).trim().toLowerCase())
    .filter(Boolean);
}

function uniqueAllowed(value, allowed) {
  const seen = new Set();
  return normalizedIds(value).filter((id) => {
    if (!allowed.has(id) || seen.has(id)) return false;
    seen.add(id);
    return true;
  });
}

function filterClientsCsv(value) {
  return uniqueAllowed(value, ALLOWED_CLIENT_SET).join(',');
}

function filterLimitProviders(value) {
  return uniqueAllowed(value, ALLOWED_LIMIT_PROVIDER_SET).join(',');
}

function enforceDownstreamSettings(input = {}) {
  const hasClients = Object.prototype.hasOwnProperty.call(input, 'clients');
  const hasLimitProviders = Object.prototype.hasOwnProperty.call(input, 'limitProviders');
  const clients = filterClientsCsv(input.clients);
  const limitProviders = filterLimitProviders(input.limitProviders);
  return {
    ...input,
    // Local-only is a security boundary, not a UI preference. A stale settings
    // file or environment variable must not silently re-enable a Hub listener or
    // remote synchronization after an upstream merge.
    hubMode: 'local',
    hubUrl: '',
    hubHostSecret: '',
    secret: '',
    discordRpcEnabled: false,
    automaticAppUpdates: false,
    claudeWebCookie: '',
    clients: hasClients ? clients : ALLOWED_CLIENTS.join(','),
    limitProviders: hasLimitProviders ? limitProviders : ALLOWED_LIMIT_PROVIDERS.join(',')
  };
}

function scrubSessionDetail(detail) {
  if (!detail || typeof detail !== 'object') return detail;
  if (!Array.isArray(detail.exchanges)) return detail;
  return {
    ...detail,
    exchanges: detail.exchanges.map((exchange) => ({
      ...exchange,
      // Keep the upstream shape so the renderer remains compatible, but never
      // pass user prompt text across the session-detail boundary.
      promptPreview: ''
    }))
  };
}

module.exports = {
  ALLOWED_CLIENTS,
  ALLOWED_LIMIT_PROVIDERS,
  DOWNSTREAM_POLICY,
  enforceDownstreamSettings,
  filterClientsCsv,
  filterLimitProviders,
  scrubSessionDetail
};
