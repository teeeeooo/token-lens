'use strict';

const { ALLOWED_LIMIT_PROVIDERS } = require('./downstreamPolicy');

// Token Lens intentionally exposes limits only for the focused provider set.
// Provider implementations remain upstream-compatible in the repository, but
// this shared list is the validation boundary used by the limits runtime.
const LIMIT_PROVIDER_IDS = Object.freeze([...ALLOWED_LIMIT_PROVIDERS]);

// Collection client ids normally match their Limits provider id. Keep the
// upstream exceptions documented for easy future patch review, although none
// are reachable through Token Lens's active client allowlist.
const LIMIT_PROVIDER_BY_CLIENT = Object.freeze({
  micode: 'mimo',
  zcode: 'zai',
  qodercn: 'qoder'
});

const LIMIT_WINDOW_METRICS = Object.freeze(['credits', 'spend']);
const VALID_LIMIT_WINDOW_METRICS = new Set(LIMIT_WINDOW_METRICS);

function limitProvidersForDetectedClients(clientHealth) {
  const clients = clientHealth?.clients;
  if (!clients || typeof clients !== 'object' || Array.isArray(clients)) return [];
  const detectedProviders = new Set();
  for (const [client, health] of Object.entries(clients)) {
    if (health?.source?.state !== 'detected') continue;
    const clientId = String(client).trim().toLowerCase();
    detectedProviders.add(LIMIT_PROVIDER_BY_CLIENT[clientId] || clientId);
  }
  return LIMIT_PROVIDER_IDS.filter((provider) => detectedProviders.has(provider));
}

module.exports = {
  LIMIT_PROVIDER_IDS,
  LIMIT_WINDOW_METRICS,
  VALID_LIMIT_WINDOW_METRICS,
  limitProvidersForDetectedClients
};
