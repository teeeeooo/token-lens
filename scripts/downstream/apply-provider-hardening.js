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

replaceExactlyOnce(
  'function mapCodexRateLimitsToProvider(payload, meta = {}) {',
  `function codexIndividualCreditWindow(rateLimits, canonicalLimitId) {
  const individualLimit = rateLimits?.individualLimit ?? rateLimits?.individual_limit;
  if (!individualLimit || typeof individualLimit !== 'object') return null;
  const limit = Number(individualLimit.limit);
  const used = Number(individualLimit.used);
  if (!Number.isFinite(limit) || limit <= 0 || !Number.isFinite(used) || used < 0) return null;
  const remainingPercent = Number(individualLimit.remainingPercent ?? individualLimit.remaining_percent);
  const usedPercent = Number.isFinite(remainingPercent)
    ? Math.max(0, Math.min(100, 100 - remainingPercent))
    : undefined;
  return {
    kind: 'billing',
    metric: 'credits',
    label: 'Monthly',
    limitId: canonicalLimitId,
    used,
    limit,
    remaining: Math.max(0, limit - used),
    ...(usedPercent === undefined ? {} : { usedPercent }),
    resetsAt: individualLimit.resetsAt ?? individualLimit.resets_at
  };
}

function mapCodexRateLimitsToProvider(payload, meta = {}) {`,
  'Codex individual credit window helper'
);

replaceExactlyOnce(
  "  const canonicalLimitId = String(rateLimits.limitId ?? rateLimits.limit_id ?? 'codex').trim() || 'codex';\n  const windows = [];",
  "  const canonicalLimitId = String(rateLimits.limitId ?? rateLimits.limit_id ?? 'codex').trim() || 'codex';\n  const individualCreditWindow = codexIndividualCreditWindow(rateLimits, canonicalLimitId);\n  const windows = [];",
  'Codex individual credit window selection'
);

replaceExactlyOnce(
  "    const kind = codexWindowKind(key, window);\n    windows.push({",
  "    const kind = codexWindowKind(key, window);\n    // Business workspaces can expose the effective monthly spend-control limit\n    // separately from the ordinary rate-limit lanes. When present it is the\n    // authoritative Monthly row, so do not render a second generic billing lane.\n    if (individualCreditWindow && kind === 'billing') continue;\n    windows.push({",
  'Codex generic monthly replacement'
);

replaceExactlyOnce(
  "  }\n  windows.push(...codexAdditionalRateLimitWindows(payload));\n  return normalizeLimitProvider({",
  "  }\n  if (individualCreditWindow) windows.push(individualCreditWindow);\n  windows.push(...codexAdditionalRateLimitWindows(payload));\n  return normalizeLimitProvider({",
  'Codex individual credit window append'
);

if (changed) {
  fs.writeFileSync(target, source);
  console.log('Token Lens provider hardening materialized: src/shared/limitCollector.js');
} else {
  console.log('Token Lens provider hardening is already materialized');
}
