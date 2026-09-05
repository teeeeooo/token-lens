import { getQuotaReport, getSessionMetadata, getUsageReport, getUsageSinceReport } from './backend.js';

const DISJOINT_REASONING_CLIENTS = new Set(['codex']);
const DEFAULT_LIMIT_REFRESH_MS = 5 * 60 * 1000;
const TODAY_CACHE_MS = 30 * 1000;
const MONTH_CACHE_MS = 2 * 60 * 1000;
const ALL_TIME_CACHE_MS = 5 * 60 * 1000;
const DERIVED_CACHE_MS = 60 * 1000;
const SESSION_METADATA_CACHE_MS = 60 * 1000;

function finite(value) {
  const number = Number(value);
  return Number.isFinite(number) ? number : 0;
}

function tokenTotal(entry) {
  const base = finite(entry?.input)
    + finite(entry?.output)
    + finite(entry?.cacheRead)
    + finite(entry?.cacheWrite);
  return base + (DISJOINT_REASONING_CLIENTS.has(entry?.client) ? finite(entry?.reasoning) : 0);
}

function publicOutput(entry) {
  return finite(entry?.output)
    + (DISJOINT_REASONING_CLIENTS.has(entry?.client) ? finite(entry?.reasoning) : 0);
}

function emptyPeriod() {
  return {
    capabilities: { tokenComponents: true },
    totalTokens: 0,
    costUsd: 0,
    cacheReadTokens: 0,
    cacheWriteTokens: 0,
    outputTokens: 0,
    unclassifiedTokens: 0,
    timedTokens: 0,
    timedOutputTokens: 0,
    timedDurationMs: 0,
    clients: {},
    clientCosts: {},
    clientCacheReads: {},
    clientCacheWrites: {},
    clientOutputs: {},
    clientUnclassifiedTokens: {},
    models: {},
    modelCosts: {},
    modelCacheReads: {},
    modelCacheWrites: {},
    modelOutputs: {},
    modelUnclassifiedTokens: {},
    clientModels: {},
    clientModelCosts: {},
    projects: {},
    sessions: {},
  };
}

function addNumber(map, key, value) {
  const amount = finite(value);
  if (!key || amount <= 0) return;
  map[key] = finite(map[key]) + amount;
}

function sessionKey(client, sessionId) {
  return `${client}:${sessionId}`;
}

function emptySession(client, sessionId) {
  return {
    client,
    sessionId,
    totalTokens: 0,
    costUsd: 0,
    messageCount: 0,
    inputTokens: 0,
    outputTokens: 0,
    cacheReadTokens: 0,
    cacheWriteTokens: 0,
    reasoningTokens: 0,
    startedAt: '',
    lastUsedAt: '',
    projectId: '',
    projectLabel: '',
    models: {},
    modelCosts: {},
    providers: {},
    sessionTitle: '',
  };
}

function addSession(period, entry, total) {
  const client = String(entry?.client || '').trim().toLowerCase();
  const sessionId = String(entry?.sessionId || '').trim();
  if (!client || !sessionId || total <= 0) return;
  const key = sessionKey(client, sessionId);
  const session = period.sessions[key] ||= emptySession(client, sessionId);
  const model = String(entry?.model || '').trim().toLowerCase();
  const provider = String(entry?.provider || '').trim().toLowerCase();
  session.totalTokens += total;
  session.costUsd += finite(entry?.cost);
  session.messageCount += Math.max(0, Math.round(finite(entry?.messageCount)));
  session.inputTokens += Math.max(0, Math.round(finite(entry?.input)));
  session.outputTokens += Math.max(0, Math.round(finite(entry?.output)));
  session.cacheReadTokens += Math.max(0, Math.round(finite(entry?.cacheRead)));
  session.cacheWriteTokens += Math.max(0, Math.round(finite(entry?.cacheWrite)));
  session.reasoningTokens += Math.max(0, Math.round(finite(entry?.reasoning)));
  addNumber(session.models, model, total);
  addNumber(session.modelCosts, model, entry?.cost);
  addNumber(session.providers, provider, total);
}

export function applySessionMetadata(periods, report) {
  const metadataByKey = new Map((report?.sessions || []).map((item) => [
    sessionKey(String(item?.client || '').trim().toLowerCase(), String(item?.sessionId || '').trim()),
    item,
  ]));
  for (const period of Object.values(periods || {})) {
    for (const [key, session] of Object.entries(period?.sessions || {})) {
      const metadata = metadataByKey.get(key);
      if (!metadata) continue;
      session.sessionTitle = String(metadata.sessionTitle || '').trim();
      session.projectLabel = String(metadata.projectLabel || '').trim();
    }
  }
  return periods;
}

function sessionMetadataRefs(periods) {
  const refs = new Map();
  for (const period of Object.values(periods || {})) {
    for (const session of Object.values(period?.sessions || {})) {
      const client = String(session?.client || '').trim().toLowerCase();
      const sessionId = String(session?.sessionId || '').trim();
      if (!client || !sessionId || !['codex', 'claude', 'gemini'].includes(client)) continue;
      refs.set(sessionKey(client, sessionId), { client, sessionId });
    }
  }
  return Array.from(refs.values()).sort((a, b) =>
    a.client.localeCompare(b.client) || a.sessionId.localeCompare(b.sessionId));
}

export function usageReportToCompatPeriod(report) {
  const period = emptyPeriod();
  for (const entry of report?.entries || []) {
    const client = String(entry?.client || '').trim().toLowerCase();
    const model = String(entry?.model || '').trim().toLowerCase();
    const total = Math.max(0, Math.round(tokenTotal(entry)));
    const cost = Math.max(0, finite(entry?.cost));
    const cacheRead = Math.max(0, Math.round(finite(entry?.cacheRead)));
    const cacheWrite = Math.max(0, Math.round(finite(entry?.cacheWrite)));
    const output = Math.max(0, Math.round(publicOutput(entry)));

    period.totalTokens += total;
    period.costUsd += cost;
    period.cacheReadTokens += cacheRead;
    period.cacheWriteTokens += cacheWrite;
    period.outputTokens += output;
    addNumber(period.clients, client, total);
    addNumber(period.clientCosts, client, cost);
    addNumber(period.clientCacheReads, client, cacheRead);
    addNumber(period.clientCacheWrites, client, cacheWrite);
    addNumber(period.clientOutputs, client, output);
    addNumber(period.models, model, total);
    addNumber(period.modelCosts, model, cost);
    addNumber(period.modelCacheReads, model, cacheRead);
    addNumber(period.modelCacheWrites, model, cacheWrite);
    addNumber(period.modelOutputs, model, output);
    if (client && model) {
      period.clientModels[client] ||= {};
      period.clientModelCosts[client] ||= {};
      addNumber(period.clientModels[client], model, total);
      addNumber(period.clientModelCosts[client], model, cost);
    }
    addSession(period, entry, total);
  }

  period.totalTokens = Math.round(period.totalTokens);
  period.costUsd = Number(period.costUsd.toFixed(6));
  period.cacheReadTokens = Math.round(period.cacheReadTokens);
  period.cacheWriteTokens = Math.round(period.cacheWriteTokens);
  period.outputTokens = Math.round(period.outputTokens);
  return period;
}

function compatibilityWindow(window) {
  return {
    kind: window.kind,
    metric: window.metric || 'quota',
    ...(window.additional ? { additional: true } : {}),
    label: String(window.label || ''),
    used: window.used ?? null,
    limit: window.limit ?? null,
    remaining: window.remaining ?? null,
    usedPercent: window.usedPercent ?? null,
    remainingPercent: window.remainingPercent ?? null,
    resetsAt: window.resetsAt || null,
    currency: window.currency || null,
    showMeter: window.showMeter !== false,
    source: window.source || '',
  };
}

export function quotaReportToCompatLimits(report) {
  const generatedAt = finite(report?.generatedAtMs);
  return {
    updatedAt: generatedAt > 0 ? new Date(generatedAt).toISOString() : '',
    refreshMs: DEFAULT_LIMIT_REFRESH_MS,
    providers: (report?.providers || []).map((provider) => ({
      provider: provider.provider,
      accountKey: '',
      accountLabel: '',
      planLabel: String(provider.plan || ''),
      accountName: '',
      accountEmail: String(provider.accountEmail || ''),
      workspaceKind: '',
      status: 'ok',
      source: 'api',
      sourceDetail: '',
      updatedAt: generatedAt > 0 ? new Date(generatedAt).toISOString() : '',
      windows: (provider.windows || []).map(compatibilityWindow),
      balanceUsd: null,
      balance: null,
      resetCredits: provider.resetCredits || null,
      region: '',
    })),
  };
}

function latestGeneratedAt(reports) {
  return Math.max(0, ...reports.map((report) => finite(report?.generatedAtMs)));
}

export function createStatsLoader({
  usage = getUsageReport,
  usageSince = getUsageSinceReport,
  quota = getQuotaReport,
  sessionMetadata = async () => ({ sessions: [] }),
  now = () => Date.now(),
} = {}) {
  const cache = new Map();

  async function cached(key, ttlMs, loader, force) {
    const current = cache.get(key);
    const timestamp = now();
    if (!force && current && timestamp - current.at < ttlMs) return current.value;
    const value = await loader();
    cache.set(key, { at: timestamp, value });
    return value;
  }

  return async function getStats(options = {}) {
    const force = options?.force === true;
    const includeSessionMetadata = options?.includeSessionMetadata === true;
    const derived = options?.derived && typeof options.derived === 'object' ? options.derived : null;
    // Keep native scans serial. Cache slower-changing ranges so the renderer can poll cheaply.
    const today = await cached('today', TODAY_CACHE_MS, () => usage('today', 'client_session_model'), force);
    const month = await cached('month', MONTH_CACHE_MS, () => usage('month', 'client_session_model'), force);
    const allTime = await cached('allTime', ALL_TIME_CACHE_MS, () => usage('all_time', 'client_session_model'), force);
    let derivedReport = null;
    if (derived?.key && derived?.since) {
      const cacheKey = `derived:${derived.key}:${derived.since}`;
      derivedReport = await cached(cacheKey, DERIVED_CACHE_MS, () => usageSince(derived.since, 'client_session_model'), force);
    }
    const limitsReport = await cached('quota', DEFAULT_LIMIT_REFRESH_MS, quota, force);
    const reports = [today, month, allTime, limitsReport, derivedReport].filter(Boolean);
    const generatedAt = latestGeneratedAt(reports);
    const periods = {
      today: usageReportToCompatPeriod(today),
      month: usageReportToCompatPeriod(month),
      allTime: usageReportToCompatPeriod(allTime),
    };
    if (derivedReport) periods[derived.key] = usageReportToCompatPeriod(derivedReport);

    const metadataRefs = includeSessionMetadata ? sessionMetadataRefs(periods) : [];
    if (metadataRefs.length) {
      const metadataKey = metadataRefs.map((item) => `${item.client}:${item.sessionId}`).join('|');
      try {
        const metadataReport = await cached(
          `sessionMetadata:${metadataKey}`,
          SESSION_METADATA_CACHE_MS,
          () => sessionMetadata(metadataRefs),
          force,
        );
        applySessionMetadata(periods, metadataReport);
      } catch (_) {
        // Session titles are optional enrichment; usage/quota must remain available if metadata lookup fails.
      }
    }

    return {
      updatedAt: generatedAt > 0 ? new Date(generatedAt).toISOString() : new Date().toISOString(),
      periods,
      limits: quotaReportToCompatLimits(limitsReport),
      devices: [],
      historyAvailable: true,
    };
  };
}

export const getStats = createStatsLoader({ sessionMetadata: getSessionMetadata });
