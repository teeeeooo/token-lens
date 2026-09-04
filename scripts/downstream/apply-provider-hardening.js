'use strict';

const fs = require('node:fs');
const path = require('node:path');

const target = path.join(__dirname, '..', '..', 'src', 'shared', 'limitCollector.js');
let source = fs.readFileSync(target, 'utf8');
let changed = false;

function replaceExactlyOnce(before, after, label) {
  if (source.includes(after)) return;
  const first = source.indexOf(before);
  if (first < 0) throw new Error(`downstream provider patch anchor missing or changed upstream: ${label}`);
  if (source.indexOf(before, first + before.length) >= 0) {
    throw new Error(`downstream provider patch anchor is ambiguous: ${label}`);
  }
  source = source.slice(0, first) + after + source.slice(first + before.length);
  changed = true;
}

replaceExactlyOnce(
  "const { appVersion } = require('./appVersion');",
  "const { appVersion } = require('./appVersion');\nconst { DOWNSTREAM_POLICY } = require('./downstreamPolicy');",
  'downstream policy import'
);

replaceExactlyOnce(
  "async function refreshClaudeCredentials(currentCredentials, deps = {}) {\n  const platform = deps.platform || process.platform;",
  "async function refreshClaudeCredentials(currentCredentials, deps = {}) {\n  // Token Lens may read Claude Code OAuth material for quota requests, but it\n  // must not refresh/rotate that material itself. A refresh can rotate the\n  // refresh token remotely and persist a replacement locally, so it is a\n  // credential mutation even before the filesystem write.\n  if (!DOWNSTREAM_POLICY.credentialMutation) {\n    throw errorWithStatus('unauthorized', 'Token Lens treats Claude authentication as read-only');\n  }\n  const platform = deps.platform || process.platform;",
  'Claude OAuth refresh gate'
);

if (changed) {
  fs.writeFileSync(target, source);
  console.log('Token Lens provider hardening materialized: src/shared/limitCollector.js');
} else {
  console.log('Token Lens provider hardening is already materialized');
}
