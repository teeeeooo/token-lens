import assert from 'node:assert/strict';
import test from 'node:test';
import {
  createStatsLoader,
  quotaReportToCompatLimits,
  usageReportToCompatPeriod,
} from '../src/stats-compat.js';

function report(entries, generatedAtMs = 1_700_000_000_000) {
  return { entries, generatedAtMs, source: 'tokscale' };
}

test('usage compatibility preserves model, client, session, components, and cost', () => {
  const period = usageReportToCompatPeriod(report([
    {
      client: 'codex', provider: 'openai', model: 'gpt-5.6-sol', sessionId: 's1',
      input: 100, output: 20, cacheRead: 300, cacheWrite: 4, reasoning: 5,
      messageCount: 2, cost: 0.42,
    },
    {
      client: 'claude', provider: 'anthropic', model: 'claude-opus', sessionId: 's2',
      input: 2, output: 3, cacheRead: 4, cacheWrite: 0, reasoning: 7,
      messageCount: 1, cost: 0.08,
    },
  ]));

  assert.equal(period.totalTokens, 438);
  assert.equal(period.outputTokens, 28);
  assert.equal(period.cacheReadTokens, 304);
  assert.equal(period.clients.codex, 429);
  assert.equal(period.clients.claude, 9);
  assert.equal(period.models['gpt-5.6-sol'], 429);
  assert.equal(period.clientModels.codex['gpt-5.6-sol'], 429);
  assert.equal(period.costUsd, 0.5);
  const codexSession = period.sessions['codex:s1'];
  assert.equal(codexSession.totalTokens, 429);
  assert.equal(codexSession.outputTokens, 20);
  assert.equal(codexSession.reasoningTokens, 5);
  assert.equal(codexSession.models['gpt-5.6-sol'], 429);
  assert.equal(codexSession.providers.openai, 429);
});

test('quota compatibility retains canonical and additional lanes', () => {
  const limits = quotaReportToCompatLimits({
    generatedAtMs: 1_700_000_000_000,
    providers: [{
      provider: 'codex',
      plan: 'Plus',
      accountEmail: 'user@example.test',
      windows: [
        { kind: 'weekly', label: 'Weekly', metric: 'quota', additional: false, usedPercent: 40, remainingPercent: 60, showMeter: true },
        { kind: 'weekly', label: 'Gpt-reserve weekly', metric: 'quota', additional: true, usedPercent: 10, remainingPercent: 90, showMeter: true },
        {
          kind: 'billing', label: 'Monthly', metric: 'credits', additional: false,
          used: 432.762320022503, limit: 750, remaining: 317.237679977497,
          usedPercent: 58, remainingPercent: 42, currency: 'CREDITS', showMeter: true,
          source: 'codex-app-server',
        },
      ],
      resetCredits: { availableCount: 1, expirations: [] },
    }],
  });

  assert.equal(limits.providers.length, 1);
  assert.equal(limits.providers[0].planLabel, 'Plus');
  assert.equal(limits.providers[0].windows[0].additional, undefined);
  assert.equal(limits.providers[0].windows[1].additional, true);
  assert.equal(limits.providers[0].windows[2].metric, 'credits');
  assert.equal(limits.providers[0].windows[2].used, 432.762320022503);
  assert.equal(limits.providers[0].windows[2].limit, 750);
  assert.equal(limits.providers[0].windows[2].currency, 'CREDITS');
  assert.equal(limits.providers[0].windows[2].source, 'codex-app-server');
  assert.equal(limits.providers[0].resetCredits.availableCount, 1);
});

test('quota diagnostics make empty provider failures actionable without changing healthy rows', () => {
  const limits = quotaReportToCompatLimits({
    generatedAtMs: 1_700_000_000_000,
    providers: [
      { provider: 'codex', diagnostic: 'Codex App Server: CLI not found', windows: [] },
      { provider: 'claude', diagnostic: null, windows: [] },
    ],
  });
  assert.equal(limits.providers[0].status, 'unavailable');
  assert.equal(limits.providers[0].diagnostic, 'Codex App Server: CLI not found');
  assert.equal(limits.providers[1].status, 'unavailable');
  assert.equal(limits.providers[1].diagnostic, '');
});

test('stale quota keeps last-good windows visible with an explicit stale status', () => {
  const limits = quotaReportToCompatLimits({
    generatedAtMs: 1_700_000_000_000,
    providers: [{
      provider: 'claude',
      status: 'stale',
      diagnostic: 'Stale Claude quota · Claude usage rate limited',
      windows: [{ kind: 'session', metric: 'quota', label: '5h', remainingPercent: 62 }],
    }],
  });
  assert.equal(limits.providers[0].status, 'stale');
  assert.equal(limits.providers[0].windows[0].remainingPercent, 62);
});

test('progressive bootstrap returns Today before quota and slower ranges', async () => {
  const calls = [];
  const usage = async (period) => {
    calls.push(`usage:${period}`);
    return report([], period === 'today' ? 1000 : 2000);
  };
  const quota = async () => {
    calls.push('quota');
    return { generatedAtMs: 3000, providers: [], source: 'tokscale' };
  };
  const getStats = createStatsLoader({ usage, quota });

  const bootstrap = await getStats.getBootstrapStats();
  assert.deepEqual(calls, ['usage:today']);
  assert.deepEqual(Object.keys(bootstrap.periods), ['today']);
  assert.equal(bootstrap.limits.providers.length, 0);

  const quotaPatch = await getStats.getQuotaLimits();
  assert.deepEqual(calls, ['usage:today', 'quota']);
  assert.equal(quotaPatch.limits.providers.length, 0);
});

test('background preload shares an in-flight Month scan with an early period request', async () => {
  const calls = [];
  let releaseMonth;
  const monthGate = new Promise((resolve) => { releaseMonth = resolve; });
  const usage = async (period) => {
    calls.push(period);
    if (period === 'month') await monthGate;
    return report([], period === 'month' ? 2000 : 3000);
  };
  const getStats = createStatsLoader({ usage, quota: async () => ({ generatedAtMs: 1, providers: [] }) });
  const progress = [];

  const preload = getStats.preloadSlowUsage({ onProgress: (period) => progress.push(period) });
  const requestedMonth = getStats.getPeriodStats('month');
  await Promise.resolve();
  assert.equal(calls.filter((period) => period === 'month').length, 1);
  releaseMonth();

  const month = await requestedMonth;
  const slow = await preload;
  assert.equal(month.period, 'month');
  assert.equal(calls.filter((period) => period === 'month').length, 1);
  assert.equal(calls.filter((period) => period === 'all_time').length, 1);
  assert.deepEqual(Object.keys(slow.periods), ['month', 'allTime']);
  assert.deepEqual(progress, ['month', 'allTime']);
  assert.equal('limits' in slow, false);
});

test('getStats compatibility loader keeps tokScale scans serial and exposes v1 period keys', async () => {
  const calls = [];
  const usage = async (period, grouping) => {
    calls.push(`usage:${period}:${grouping}`);
    return report([], period === 'today' ? 1000 : period === 'month' ? 2000 : 3000);
  };
  const quota = async () => {
    calls.push('quota');
    return { generatedAtMs: 4000, providers: [], source: 'tokscale' };
  };
  const getStats = createStatsLoader({ usage, quota });

  const stats = await getStats();
  assert.deepEqual(calls, [
    'quota',
    'usage:today:client_session_model',
    'usage:month:client_session_model',
    'usage:all_time:client_session_model',
  ]);
  assert.deepEqual(Object.keys(stats.periods), ['today', 'month', 'allTime']);
  assert.equal(stats.historyAvailable, true);
  assert.deepEqual(stats.devices, []);
  assert.equal(stats.limits.providers.length, 0);
});

test('getStats caches slower ranges and refreshes today independently', async () => {
  const calls = [];
  let clock = 1_000_000;
  const usage = async (period) => {
    calls.push(`usage:${period}`);
    return report([], clock);
  };
  const quota = async () => {
    calls.push('quota');
    return { generatedAtMs: clock, providers: [], source: 'tokscale' };
  };
  const getStats = createStatsLoader({ usage, quota, now: () => clock });
  await getStats();
  clock += 31_000;
  await getStats();
  assert.deepEqual(calls, [
    'quota', 'usage:today', 'usage:month', 'usage:all_time',
    'usage:today',
  ]);
});

test('getStats rechecks only recovering providers after 30s without rerunning full quota', async () => {
  let clock = 1_000_000;
  let quotaCalls = 0;
  let recoveryCalls = 0;
  const usage = async () => report([], clock);
  const pending = () => ({
    generatedAtMs: clock,
    providers: [{
      provider: 'claude',
      recoveryState: 'pending',
      diagnostic: 'Claude credential is unavailable; Claude CLI credential refresh started in background',
      windows: [],
    }],
  });
  const quota = async () => { quotaCalls += 1; return pending(); };
  const quotaRecovery = async () => {
    recoveryCalls += 1;
    return { generatedAtMs: clock, providers: [{ provider: 'claude', diagnostic: null, windows: [] }] };
  };
  const getStats = createStatsLoader({ usage, quota, quotaRecovery, now: () => clock });

  const first = await getStats();
  assert.equal(first.limits.refreshMs, 30_000);
  assert.equal(quotaCalls, 1);
  assert.equal(recoveryCalls, 0);

  clock += 29_000;
  await getStats();
  assert.equal(quotaCalls, 1);
  assert.equal(recoveryCalls, 0);

  clock += 2_000;
  const recovered = await getStats();
  assert.equal(quotaCalls, 1);
  assert.equal(recoveryCalls, 1);
  assert.equal(recovered.limits.refreshMs, 5 * 60 * 1000);

  clock += 31_000;
  await getStats();
  assert.equal(quotaCalls, 1);
  assert.equal(recoveryCalls, 1);
});

test('provider-only recovery does not postpone the five-minute full quota refresh', async () => {
  let clock = 2_000_000;
  let quotaCalls = 0;
  let recoveryCalls = 0;
  const usage = async () => report([], clock);
  const pending = () => ({
    generatedAtMs: clock,
    providers: [{
      provider: 'gemini',
      recoveryState: 'pending',
      diagnostic: 'Gemini credential unavailable; Gemini CLI credential refresh cooling down; retry in about 240s',
      windows: [],
    }],
  });
  const quota = async () => { quotaCalls += 1; return pending(); };
  const quotaRecovery = async () => { recoveryCalls += 1; return pending(); };
  const getStats = createStatsLoader({ usage, quota, quotaRecovery, now: () => clock });

  await getStats.getQuotaLimits();
  clock += 31_000;
  await getStats.getQuotaLimits();
  assert.equal(quotaCalls, 1);
  assert.equal(recoveryCalls, 1);

  clock += 5 * 60 * 1000 - 31_000;
  await getStats.getQuotaLimits();
  assert.equal(quotaCalls, 2);
  assert.equal(recoveryCalls, 1);
});

test('failed provider-only recovery remains throttled to the 30s recovery cadence', async () => {
  let clock = 3_000_000;
  let quotaCalls = 0;
  let recoveryCalls = 0;
  const usage = async () => report([], clock);
  const quota = async () => {
    quotaCalls += 1;
    return {
      generatedAtMs: clock,
      providers: [{
        provider: 'claude',
        recoveryState: 'pending',
        diagnostic: 'Claude CLI credential refresh already in progress',
        windows: [],
      }],
    };
  };
  const quotaRecovery = async () => { recoveryCalls += 1; throw new Error('transient recovery command failure'); };
  const getStats = createStatsLoader({ usage, quota, quotaRecovery, now: () => clock });

  await getStats.getQuotaLimits();
  clock += 31_000;
  await getStats.getQuotaLimits();
  assert.equal(quotaCalls, 1);
  assert.equal(recoveryCalls, 1);

  clock += 1_000;
  await getStats.getQuotaLimits();
  assert.equal(recoveryCalls, 1);

  clock += 30_000;
  await getStats.getQuotaLimits();
  assert.equal(recoveryCalls, 2);
});

test('quota compatibility keeps 30s polling during CLI refresh backoff', () => {
  const limits = quotaReportToCompatLimits({
    generatedAtMs: 1_700_000_000_000,
    providers: [{
      provider: 'gemini',
      recoveryState: 'pending',
      diagnostic: 'Gemini credential unavailable; Gemini CLI credential refresh cooling down; retry in about 240s',
      windows: [],
    }],
  });
  assert.equal(limits.refreshMs, 30_000);
});

test('getStats exposes a cached derived period from an explicit since date', async () => {
  const calls = [];
  const usage = async (period) => report([], period === 'today' ? 1000 : 2000);
  const usageSince = async (since, grouping) => {
    calls.push(`${since}:${grouping}`);
    return report([{ client: 'codex', model: 'gpt-5.6-sol', input: 10 }], 3000);
  };
  const quota = async () => ({ generatedAtMs: 4000, providers: [] });
  const getStats = createStatsLoader({ usage, usageSince, quota, now: () => 10_000 });

  const options = { derived: { key: 'last7', since: '2026-08-30' } };
  const first = await getStats(options);
  const second = await getStats(options);
  assert.deepEqual(calls, ['2026-08-30:client_session_model']);
  assert.equal(first.periods.last7.totalTokens, 10);
  assert.equal(second.periods.last7.totalTokens, 10);
});

test('forced refresh bypasses all stats caches', async () => {
  let calls = 0;
  const usage = async () => { calls += 1; return report([]); };
  const quota = async () => { calls += 1; return { generatedAtMs: 1, providers: [] }; };
  const getStats = createStatsLoader({ usage, quota, now: () => 1000 });
  await getStats();
  await getStats({ force: true });
  assert.equal(calls, 8);
});


test('getStats batch-decorates supported sessions with provider-owned metadata', async () => {
  const entry = {
    client: 'codex', provider: 'openai', model: 'gpt-5.6-sol', sessionId: 'rollout-1',
    input: 10, output: 2, cacheRead: 0, cacheWrite: 0, reasoning: 1, messageCount: 1, cost: 0.1,
  };
  const usage = async () => report([entry]);
  const quota = async () => ({ generatedAtMs: 4000, providers: [] });
  const metadataCalls = [];
  const sessionMetadata = async (refs) => {
    metadataCalls.push(refs);
    return { sessions: [{
      client: 'codex', sessionId: 'rollout-1', sessionTitle: 'Provider title', projectLabel: 'token-lens',
    }] };
  };
  const getStats = createStatsLoader({ usage, quota, sessionMetadata, now: () => 10_000 });
  const stats = await getStats({ includeSessionMetadata: true });

  assert.deepEqual(metadataCalls, [[{ client: 'codex', sessionId: 'rollout-1' }]]);
  for (const period of Object.values(stats.periods)) {
    assert.equal(period.sessions['codex:rollout-1'].sessionTitle, 'Provider title');
    assert.equal(period.sessions['codex:rollout-1'].projectLabel, 'token-lens');
  }
});


test('getStats avoids provider session metadata I/O outside the Sessions view', async () => {
  const usage = async () => report([{
    client: 'codex', provider: 'openai', model: 'gpt-5', sessionId: 's1', input: 1,
  }]);
  const quota = async () => ({ generatedAtMs: 1, providers: [] });
  let metadataCalls = 0;
  const sessionMetadata = async () => { metadataCalls += 1; return { sessions: [] }; };
  const getStats = createStatsLoader({ usage, quota, sessionMetadata, now: () => 1_000 });
  await getStats();
  assert.equal(metadataCalls, 0);
});


function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

const countedReport = (input) => report([{ client: 'codex', model: 'test', input }]);

test('older in-flight usage cannot overwrite a completed forced refresh', async () => {
  const old = deferred();
  let calls = 0;
  const stats = createStatsLoader({ usage: () => ++calls === 1 ? old.promise : countedReport(20) });
  const pending = stats.getPeriodStats('today');
  await Promise.resolve();
  assert.equal((await stats.getPeriodStats('today', { force: true })).value.totalTokens, 20);
  old.resolve(countedReport(10));
  await pending;
  assert.equal((await stats.getPeriodStats('today')).value.totalTokens, 20);
  assert.equal(calls, 2);
});

test('ordinary usage readers join the latest forced request rather than stale cache', async () => {
  const fresh = deferred();
  let calls = 0;
  const stats = createStatsLoader({ usage: () => ++calls === 1 ? countedReport(10) : fresh.promise });
  await stats.getPeriodStats('today');
  const forced = stats.getPeriodStats('today', { force: true });
  await Promise.resolve();
  const ordinary = stats.getPeriodStats('today');
  fresh.resolve(countedReport(20));
  assert.equal((await forced).value.totalTokens, 20);
  assert.equal((await ordinary).value.totalTokens, 20);
  assert.equal(calls, 2);
});

test('failed latest request does not let superseded usage poison the cache', async () => {
  const old = deferred();
  let calls = 0;
  const stats = createStatsLoader({ usage: () => {
    calls += 1;
    if (calls === 1) return old.promise;
    if (calls === 2) throw new Error('synthetic failure');
    return countedReport(30);
  } });
  const pending = stats.getPeriodStats('today');
  await assert.rejects(stats.getPeriodStats('today', { force: true }), /synthetic failure/);
  old.resolve(countedReport(10));
  await pending;
  assert.equal((await stats.getPeriodStats('today')).value.totalTokens, 30);
});

test('late recovery cannot overwrite a newer full quota snapshot', async () => {
  let clock = 1_000_000;
  let calls = 0;
  const recovery = deferred();
  const started = deferred();
  const pending = { generatedAtMs: clock, providers: [{ provider: 'claude',
    recoveryState: 'pending',
    diagnostic: 'Claude CLI credential refresh cooling down', windows: [],
  }] };
  const fresh = { generatedAtMs: clock + 31_000, providers: [{ provider: 'claude',
    windows: [{ kind: 'session', remainingPercent: 80 }],
  }] };
  const stats = createStatsLoader({ now: () => clock,
    quota: async () => ++calls === 1 ? pending : fresh,
    quotaRecovery: () => { started.resolve(); return recovery.promise; },
  });
  await stats.getQuotaLimits();
  clock += 31_000;
  const older = stats.getQuotaLimits();
  await started.promise;
  await stats.getQuotaLimits({ force: true });
  recovery.resolve(pending);
  await older;
  const latest = await stats.getQuotaLimits();
  assert.equal(latest.limits.providers[0].windows[0]?.remainingPercent, 80);
  assert.equal(calls, 2);
});

test('partial refresh commits Today and quota even when Month fails', async () => {
  const patches = [];
  const stats = createStatsLoader({
    usage: async (period) => { if (period === 'month') throw new Error('private sentinel'); return countedReport(42); },
    quota: async () => ({ generatedAtMs: 42, providers: [] }),
  });
  const result = await stats({ onPatch: (patch) => patches.push(patch) });
  assert.equal(result.periods.today.totalTokens, 42);
  assert.equal(result.periods.month, undefined);
  assert.equal(result.resources.month.status, 'unavailable');
  assert.equal(result.resources.quota.status, 'ready');
  assert.ok(patches.some((patch) => patch.periods?.today));
  assert.ok(!JSON.stringify(result).includes('private sentinel'));
});

test('quota failure retains stale quota without blocking fresh usage', async () => {
  let fail = false;
  const stats = createStatsLoader({ usage: async () => countedReport(fail ? 20 : 10),
    quota: async () => { if (fail) throw new Error('private'); return { generatedAtMs: 100, providers: [{ provider: 'codex', lastSuccessAtMs: 100, windows: [{ remainingPercent: 50 }] }] }; },
  });
  await stats();
  fail = true;
  const result = await stats({ force: true });
  assert.equal(result.periods.today.totalTokens, 20);
  assert.equal(result.resources.quota.status, 'stale');
  assert.equal(result.resources.quota.lastSuccessAtMs, 100);
  assert.equal(result.limits.providers[0].status, 'stale');
});

test('Today and quota patches arrive before a delayed Month and All Time stays serial', async () => {
  const month = deferred();
  const seen = [];
  const calls = [];
  const stats = createStatsLoader({ usage: async (period) => {
    calls.push(period);
    if (period === 'month') return month.promise;
    return countedReport(7);
  }, quota: async () => ({ generatedAtMs: 1, providers: [] }) });
  const task = stats({ onPatch: (p) => seen.push(p) });
  await new Promise((resolve) => setImmediate(resolve));
  assert.ok(seen.some((p) => p.periods?.today?.totalTokens === 7));
  assert.ok(seen.some((p) => p.resources?.quota?.status === 'ready'));
  assert.deepEqual(calls, ['today', 'month']);
  month.reject(new Error('timeout'));
  const result = await task;
  assert.equal(result.resources.month.status, 'unavailable');
  assert.deepEqual(calls, ['today', 'month', 'all_time']);
});

test('superseded full refresh cannot publish late data or start later scans', async () => {
  const old = deferred();
  const patches = [];
  let first = true;
  const stats = createStatsLoader({ usage: async () => {
    if (first) { first = false; return old.promise; }
    return countedReport(20);
  }, quota: async () => ({ generatedAtMs: 1, providers: [] }) });
  const earlier = stats({ onPatch: (p) => patches.push(p) });
  await new Promise((resolve) => setImmediate(resolve));
  await stats({ force: true });
  const before = patches.length;
  old.resolve(countedReport(10));
  await earlier;
  assert.equal(patches.length, before);
  assert.equal((await stats()).periods.today.totalTokens, 20);
});

test('quota control and success timestamps are independent of report time and diagnostic text', () => {
  const providers = [
    { provider: 'codex', status: 'ready', recoveryState: 'idle', lastSuccessAtMs: 1000, windows: [{ remainingPercent: 70 }] },
    { provider: 'gemini', status: 'stale', recoveryState: 'cooldown', lastSuccessAtMs: 2000, lastAttemptAtMs: 9000, retryAtMs: 15000, windows: [{ remainingPercent: 50 }] },
  ];
  const first = quotaReportToCompatLimits({ generatedAtMs: 3000, providers });
  const later = quotaReportToCompatLimits({ generatedAtMs: 10000, providers: providers.map((p) => ({ ...p, diagnostic: '표현이 바뀌어도 동일' })) });
  assert.equal(first.refreshMs, later.refreshMs);
  assert.equal(later.refreshMs, 30000);
  assert.equal(later.providers[0].updatedAt, new Date(1000).toISOString());
  assert.equal(later.providers[1].updatedAt, new Date(2000).toISOString());
  assert.equal(later.providers[1].status, 'stale');
  assert.equal(later.providers[1].retryAtMs, 15000);
  assert.equal(later.updatedAt, first.updatedAt);
});
