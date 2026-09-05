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
    'usage:today:client_session_model',
    'usage:month:client_session_model',
    'usage:all_time:client_session_model',
    'quota',
  ]);
  assert.deepEqual(Object.keys(stats.periods), ['today', 'month', 'allTime']);
  assert.equal(stats.historyAvailable, false);
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
    'usage:today', 'usage:month', 'usage:all_time', 'quota',
    'usage:today',
  ]);
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
