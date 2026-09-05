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
      ],
      resetCredits: { availableCount: 1, expirations: [] },
    }],
  });

  assert.equal(limits.providers.length, 1);
  assert.equal(limits.providers[0].planLabel, 'Plus');
  assert.equal(limits.providers[0].windows[0].additional, undefined);
  assert.equal(limits.providers[0].windows[1].additional, true);
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
