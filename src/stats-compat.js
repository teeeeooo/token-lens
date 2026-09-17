import { getQuotaRecoveryReport, getQuotaReport, getSessionMetadata, getUsageReport, getUsageSinceReport } from './backend.js';

const DISJOINT_REASONING_CLIENTS = new Set(['codex']);
const DEFAULT_LIMIT_REFRESH_MS = 5 * 60 * 1000;
const AUTH_REFRESH_LIMIT_POLL_MS = 30 * 1000;
const TODAY_CACHE_MS = 30 * 1000;
const MONTH_CACHE_MS = 2 * 60 * 1000;
const ALL_TIME_CACHE_MS = 5 * 60 * 1000;
const DERIVED_CACHE_MS = 60 * 1000;
const SESSION_METADATA_CACHE_MS = 60 * 1000;
const DYNAMIC_CACHE_LIMIT = 64;
const SESSION_CACHE_LIMIT = 4096;
const SESSION_BATCH_SIZE = 250;

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

function quotaAuthRefreshPending(report) {
  return (report?.providers || []).some((provider) => ['pending', 'cooldown'].includes(provider.recoveryState));
}

export function quotaReportToCompatLimits(report) {
  const generatedAt = Math.max(0, ...(report?.providers || []).map((provider) => finite(provider.lastSuccessAtMs)));
  return {
    updatedAt: generatedAt > 0 ? new Date(generatedAt).toISOString() : '',
    refreshMs: quotaAuthRefreshPending(report) ? AUTH_REFRESH_LIMIT_POLL_MS : DEFAULT_LIMIT_REFRESH_MS,
    providers: (report?.providers || []).map((provider) => {
      const diagnostic = String(provider.diagnostic || '');
      const windows = provider.windows || [];
      const status = provider.status === 'stale' ? 'stale'
        : provider.status === 'unavailable' || !windows.length ? 'unavailable' : 'ok';
      return {
        provider: provider.provider,
        accountKey: '',
        accountLabel: '',
        planLabel: String(provider.plan || ''),
        accountName: '',
        accountEmail: String(provider.accountEmail || ''),
        workspaceKind: '',
        status,
        diagnostic,
        source: 'api',
        sourceDetail: '',
        updatedAt: provider.lastSuccessAtMs > 0 ? new Date(provider.lastSuccessAtMs).toISOString() : '',
        lastSuccessAtMs: provider.lastSuccessAtMs ?? null,
        lastAttemptAtMs: provider.lastAttemptAtMs ?? null,
        retryAtMs: provider.retryAtMs ?? null,
        recoveryState: provider.recoveryState || 'idle',
        windows: (provider.windows || []).map(compatibilityWindow),
        balanceUsd: null,
        balance: null,
        resetCredits: provider.resetCredits || null,
        region: '',
      };
    }),
  };
}

function latestGeneratedAt(reports) {
  return Math.max(0, ...reports.map((report) => finite(report?.generatedAtMs)));
}

export function createStatsLoader({
  usage = getUsageReport,
  usageSince = getUsageSinceReport,
  quota = getQuotaReport,
  quotaRecovery = getQuotaRecoveryReport,
  sessionMetadata = async () => ({ sessions: [] }),
  now = () => Date.now(),
  observeCache = () => {}, // Constructor-only test observation; not exposed by the facade.
} = {}) {
  const cache = new Map();
  const dynamicCache = new Map();
  const metadataCache = new Map();
  const metadataBatches = new Map();
  const inFlight = new Map();
  const invalidated = new WeakSet();
  const resourceOwners = new Map();
  let refreshGeneration = 0;

  function prune(store, limit) {
    const timestamp = now();
    for (const [key, entry] of store) {
      if (timestamp < entry.at || timestamp - entry.at >= entry.ttl) {
        store.delete(key);
        if (store === dynamicCache && inFlight.has(key)) invalidated.add(inFlight.get(key));
      }
    }
    while (store.size > limit) {
      const key = store.keys().next().value;
      store.delete(key);
      if (store === dynamicCache && inFlight.has(key)) invalidated.add(inFlight.get(key));
    }
    observeCache({ dynamic: dynamicCache.size, sessions: metadataCache.size });
  }

  function resultStore(key) { return key.startsWith('derived:') ? dynamicCache : cache; }

  async function cached(key, ttlMs, loader, force) {
    prune(dynamicCache, DYNAMIC_CACHE_LIMIT);
    prune(metadataCache, SESSION_CACHE_LIMIT);
    const store = resultStore(key);
    // Readers join the newest request, including a forced refresh, before using old cache.
    if (!force && inFlight.has(key)) return inFlight.get(key);
    const current = store.get(key);
    const timestamp = now();
    const ttl = typeof ttlMs === 'function' ? ttlMs(current?.value) : ttlMs;
    if (!force && current && timestamp - current.at >= 0 && timestamp - current.at < ttl) {
      store.delete(key);
      store.set(key, current);
      return current.value;
    }
    const pending = Promise.resolve().then(loader);
    inFlight.set(key, pending);
    try {
      const value = await pending;
      // A superseded request may finish, but must never make older data fresh again.
      if (inFlight.get(key) === pending && !invalidated.has(pending)) {
        store.set(key, { at: now(), ttl, value });
        prune(dynamicCache, DYNAMIC_CACHE_LIMIT);
      }
      return value;
    } finally {
      if (inFlight.get(key) === pending) inFlight.delete(key);
    }
  }

  function loadUsagePeriod(key, force = false) {
    if (key === 'today') return cached('today', TODAY_CACHE_MS, () => usage('today', 'client_session_model'), force);
    if (key === 'month') return cached('month', MONTH_CACHE_MS, () => usage('month', 'client_session_model'), force);
    if (key === 'allTime') return cached('allTime', ALL_TIME_CACHE_MS, () => usage('all_time', 'client_session_model'), force);
    throw new Error(`unsupported cached usage period: ${key}`);
  }

  async function loadQuotaReport(force = false) {
    const before = cache.get('quota');
    const report = await cached('quota', DEFAULT_LIMIT_REFRESH_MS, quota, force);
    const fullEntry = cache.get('quota');
    // An older full request must not change recovery state owned by a newer snapshot.
    if (fullEntry?.value !== report) return fullEntry?.value ?? report;
    const fullRefreshed = force || !before || fullEntry?.at !== before.at;

    if (!quotaAuthRefreshPending(report)) {
      cache.delete('quotaRecovery');
      inFlight.delete('quotaRecovery');
      return report;
    }

    if (fullRefreshed) {
      inFlight.delete('quotaRecovery');
      cache.set('quotaRecovery', { at: fullEntry?.at ?? now(), value: report });
      return report;
    }

    try {
      const recovered = await cached(
        'quotaRecovery',
        AUTH_REFRESH_LIMIT_POLL_MS,
        quotaRecovery,
        false,
      );
      const currentFull = cache.get('quota');
      if (currentFull !== fullEntry) return currentFull?.value ?? recovered;
      if (currentFull) cache.set('quota', { at: currentFull.at, value: recovered });
      if (!quotaAuthRefreshPending(recovered)) cache.delete('quotaRecovery');
      return recovered;
    } catch (_) {
      const currentFull = cache.get('quota');
      if (currentFull !== fullEntry) return currentFull?.value ?? report;
      cache.set('quotaRecovery', { at: now(), value: report });
      return report;
    }
  }

  function partialStats(periods = {}, reports = [], limitsReport = null) {
    const generatedAt = latestGeneratedAt(reports.filter(Boolean));
    return {
      updatedAt: generatedAt > 0 ? new Date(generatedAt).toISOString() : '',
      periods,
      ...(limitsReport ? { limits: quotaReportToCompatLimits(limitsReport) } : {}),
      devices: [],
      historyAvailable: true,
      resources: {},
    };
  }

  async function resource(key, load, convert, onPatch = () => {}) {
    const owner = {};
    resourceOwners.set(key, owner);
    const successAt = (raw) => key === 'quota'
      ? Math.max(0, ...(raw?.providers || []).map((provider) => finite(provider.lastSuccessAtMs))) || null
      : raw?.generatedAtMs || null;
    const previous = resultStore(key).get(key)?.value;
    const state = { status: 'loading', lastSuccessAtMs: successAt(previous), error: null };
    onPatch({ resources: { [key]: state } });
    let raw;
    try {
      raw = await load();
      state.status = 'ready';
      state.lastSuccessAtMs = successAt(raw);
    } catch (_) {
      raw = resultStore(key).get(key)?.value;
      state.status = raw ? 'stale' : 'unavailable';
      state.lastSuccessAtMs = successAt(raw);
      state.error = 'Resource collection failed';
    }
    if (resourceOwners.get(key) !== owner) return {};
    resourceOwners.delete(key);
    const patch = raw ? convert(raw) : {};
    if (key === 'quota' && state.status === 'stale') {
      for (const provider of patch.limits.providers) {
        provider.windows = provider.windows.filter((window) => {
          const reset = Date.parse(window.resetsAt || '');
          return !Number.isFinite(reset) || reset > now();
        });
        provider.status = provider.windows.length ? 'stale' : 'unavailable';
      }
    }
    patch.resources = { [key]: state };
    onPatch(patch);
    return patch;
  }

  function combine(target, patch) {
    Object.assign(target, {
      ...patch,
      periods: { ...target.periods, ...patch.periods },
      resources: { ...target.resources, ...patch.resources },
      updatedAt: [target.updatedAt, patch.updatedAt].filter(Boolean).sort().at(-1) || '',
    });
    return target;
  }

  function periodPatch(period, raw) {
    return partialStats({ [period]: usageReportToCompatPeriod(raw) }, [raw]);
  }

  async function getBootstrapStats(options = {}) {
    const patch = await resource('today', () => loadUsagePeriod('today', options.force === true),
      (raw) => periodPatch('today', raw));
    return combine({ ...partialStats(), limits: quotaReportToCompatLimits(null) }, patch);
  }

  async function getPeriodStats(period, options = {}) {
    const raw = await loadUsagePeriod(period, options.force === true);
    return { period, value: usageReportToCompatPeriod(raw),
      updatedAt: raw?.generatedAtMs > 0 ? new Date(raw.generatedAtMs).toISOString() : '',
      resources: { [period]: { status: 'ready', lastSuccessAtMs: raw?.generatedAtMs || null, error: null } } };
  }

  async function getQuotaLimits(options = {}) {
    return resource('quota', () => loadQuotaReport(options.force === true),
      (raw) => { const patch = partialStats({}, [], raw); patch.updatedAt = patch.limits.updatedAt; return patch; }, options.onPatch);
  }

  async function preloadSlowUsage(options = {}) {
    const result = partialStats();
    for (const period of ['month', 'allTime']) {
      const patch = await resource(period, () => loadUsagePeriod(period, options.force === true),
        (raw) => periodPatch(period, raw), options.onPatch);
      combine(result, patch);
      options.onProgress?.(period);
    }
    return result;
  }

  async function decorateSessions(refs, periods, force, current, publish) {
    // One bounded batch at a time limits provider filesystem I/O. Cache only
    // whitelisted normalized metadata per session, never entire reference lists.
    const missing = [];
    const hits = [];
    prune(metadataCache, SESSION_CACHE_LIMIT);
    for (const ref of refs) {
      const key = sessionKey(ref.client, ref.sessionId);
      const entry = metadataCache.get(key);
      if (!force && entry) {
        hits.push(entry.value);
        metadataCache.delete(key);
        metadataCache.set(key, entry);
      } else missing.push(ref);
    }
    applySessionMetadata(periods, { sessions: hits });
    for (let offset = 0; offset < missing.length && current(); offset += SESSION_BATCH_SIZE) {
      const batch = missing.slice(offset, offset + SESSION_BATCH_SIZE);
      const key = JSON.stringify(batch);
      let pending = !force && metadataBatches.get(key);
      if (!pending) {
        pending = Promise.resolve().then(() => sessionMetadata(batch));
        metadataBatches.set(key, pending);
      }
      try {
        const report = await pending;
        if (!current()) return;
        const allowed = new Set(batch.map((ref) => sessionKey(ref.client, ref.sessionId)));
        const normalized = new Map();
        for (const item of report?.sessions || []) {
          const key = sessionKey(item.client, item.sessionId);
          if (!allowed.has(key)) continue;
          normalized.set(key, { client: item.client, sessionId: item.sessionId,
            sessionTitle: String(item.sessionTitle || '').trim(), projectLabel: String(item.projectLabel || '').trim() });
        }
        // Cache safe empty metadata too so absent titles do not trigger repeated I/O.
        const sessions = batch.map((ref) => normalized.get(sessionKey(ref.client, ref.sessionId)) || { ...ref, sessionTitle: '', projectLabel: '' });
        if (metadataBatches.get(key) === pending) {
          for (const item of sessions) metadataCache.set(sessionKey(item.client, item.sessionId),
            { at: now(), ttl: SESSION_METADATA_CACHE_MS, value: item });
          prune(metadataCache, SESSION_CACHE_LIMIT);
        }
        applySessionMetadata(periods, { sessions });
        publish({ periods });
      } catch (_) { /* Isolate failed batches; retain basename/id fallback. */ }
      finally { if (metadataBatches.get(key) === pending) metadataBatches.delete(key); }
    }
    if (current()) publish({ periods });
  }

  async function getStats(options = {}) {
    const generation = ++refreshGeneration;
    const force = options.force === true;
    const result = partialStats();
    const publish = (patch) => {
      if (generation !== refreshGeneration) return;
      combine(result, patch);
      options.onPatch?.(patch);
    };
    // Quota is independent; disk-heavy usage scans remain serial and Today-first.
    const quotaTask = getQuotaLimits({ force, onPatch: publish });
    for (const period of ['today', 'month', 'allTime']) {
      if (generation !== refreshGeneration) break;
      await resource(period, () => loadUsagePeriod(period, force), (raw) => periodPatch(period, raw), publish);
    }
    const derived = options.derived;
    if (generation === refreshGeneration && derived?.key && derived?.since) {
      const key = `derived:${derived.key}:${derived.since}`;
      await resource(key, () => cached(key, DERIVED_CACHE_MS,
        () => usageSince(derived.since, 'client_session_model'), force),
      (raw) => periodPatch(derived.key, raw), (patch) => {
        if (patch.resources?.[key]) patch.resources = { [derived.key]: patch.resources[key] };
        publish(patch);
      });
    }
    await quotaTask;
    const selected = options.period || 'today';
    const refs = options.includeSessionMetadata ? sessionMetadataRefs({ selected: result.periods[selected] }) : [];
    if (refs.length && generation === refreshGeneration) {
      await decorateSessions(refs, result.periods, force, () => generation === refreshGeneration, publish);
    }
    return result;
  }

  getStats.getBootstrapStats = getBootstrapStats;
  getStats.getPeriodStats = getPeriodStats;
  getStats.getQuotaLimits = getQuotaLimits;
  getStats.preloadSlowUsage = preloadSlowUsage;
  return getStats;
}

const statsLoader = createStatsLoader({ sessionMetadata: getSessionMetadata });
export const getStats = statsLoader;
export const getBootstrapStats = statsLoader.getBootstrapStats;
export const getPeriodStats = statsLoader.getPeriodStats;
export const getQuotaLimits = statsLoader.getQuotaLimits;
export const preloadSlowUsage = statsLoader.preloadSlowUsage;
