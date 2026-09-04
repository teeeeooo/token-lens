'use strict';

const { ALLOWED_CLIENTS, filterClientsCsv } = require('./downstreamPolicy');

// Token Lens has a deliberately narrow execution and presentation surface.
// Upstream provider implementations remain in the source tree for selective
// patching, but they are not advertised or registered by this downstream build.
const PARSE_LOCAL_CLIENTS = Object.freeze([]);
const DEFAULT_CLIENTS = ALLOWED_CLIENTS.join(',');
const KNOWN_CLIENTS = DEFAULT_CLIENTS;

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
