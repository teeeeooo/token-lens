import test from 'node:test';
import assert from 'node:assert/strict';
import {
  formatCompact,
  homeQuotaWindows,
  formatQuotaCount,
  modelRows,
  modelVendorFor,
  quotaRows,
  quotaWindowLabel,
  sessionRows,
  toolRows,
} from '../src/renderer-model.js';

const period = {
  totalTokens: 1_000,
  models: { 'gpt-5.6-sol': 700, 'claude-opus-4-7': 300 },
  modelCosts: { 'gpt-5.6-sol': 7, 'claude-opus-4-7': 6 },
  clients: { codex: 700, claude: 300 },
  clientCosts: { codex: 7, claude: 6 },
  sessions: {
    'codex:rollout-1': {
      client: 'codex', sessionId: 'rollout-1', totalTokens: 700, costUsd: 7,
      messageCount: 12, models: { 'gpt-5.6-sol': 700 },
      sessionTitle: 'Restore tray behavior', projectLabel: 'token-lens',
    },
  },
};
test('renderer rows preserve v1 ranking semantics for supported tools', () => {
  const models = modelRows(period);
  assert.equal(models[0].name, 'gpt-5.6-sol');
  assert.equal(models[0].share, 0.7);
  assert.equal(models[0].iconClass, 'row-icon-codex');

  const tools = toolRows(period);
  assert.deepEqual(tools.map((row) => row.key), ['codex', 'claude']);
  assert.equal(tools[1].name, 'Claude Code');

  const sessions = sessionRows(period);
  assert.equal(sessions.length, 1);
  assert.equal(sessions[0].name, 'Restore tray behavior');
  assert.equal(sessions[0].detail, 'Codex · gpt-5.6-sol · 12 msgs');
});

test('compact formatter keeps dashboard-scale labels', () => {
  assert.equal(formatCompact(999), '999');
  assert.equal(formatCompact(12_300), '12.3K');
  assert.equal(formatCompact(12_300_000), '12.3M');
});
test('quota rows keep the supported-provider product surface and additional lanes', () => {
  const rows = quotaRows({ providers: [{
    provider: 'codex', planLabel: 'Plus', status: 'ok',
    windows: [
      { kind: 'session', remainingPercent: 35, label: '5h' },
      { kind: 'weekly', remainingPercent: 0, label: 'Weekly' },
      { kind: 'weekly', additional: true, remainingPercent: 88, label: 'GPT Reserve weekly' },
      {
        kind: 'billing', metric: 'credits', label: 'Monthly', currency: 'CREDITS',
        used: 432.762320022503, limit: 750, remaining: 317.237679977497,
        usedPercent: 58, remainingPercent: 42, source: 'codex-app-server',
      },
    ],
    resetCredits: { availableCount: 1 },
  }] });

  assert.deepEqual(rows.map((row) => row.providerId), ['codex', 'claude', 'gemini', 'antigravity']);
  assert.equal(rows[0].plan, 'Plus');
  assert.equal(rows[0].windows[2].additional, true);
  assert.equal(rows[0].windows[3].metric, 'credits');
  assert.equal(rows[0].windows[3].currency, 'CREDITS');
  assert.equal(rows[0].windows[3].used, 432.762320022503);
  assert.equal(rows[0].windows[3].source, 'codex-app-server');
  assert.equal(formatQuotaCount(rows[0].windows[3]), '317.24/750');
  assert.equal(rows[0].resetCredits.availableCount, 1);
  assert.equal(rows[1].status, 'unavailable');
  assert.equal(rows[2].status, 'unavailable');
  assert.equal(rows[3].status, 'unavailable');
  assert.equal(quotaWindowLabel(rows[0].windows[0]), '5-hour');
  assert.equal(quotaWindowLabel(rows[0].windows[2]), 'GPT Reserve weekly');
});


test('session rows fall back to project label and then session id without transcript-derived text', () => {
  const project = sessionRows({ sessions: {
    'claude:s2': { client: 'claude', sessionId: 's2', projectLabel: 'predictor', totalTokens: 10, models: { opus: 10 } },
  } });
  assert.equal(project[0].name, 'predictor');
  assert.equal(project[0].detail, 'Claude Code · opus');

  const idOnly = sessionRows({ sessions: {
    'codex:s3': { client: 'codex', sessionId: 's3', totalTokens: 10, models: { 'gpt-5': 10 } },
  } });
  assert.equal(idOnly[0].name, 's3');
  assert.equal(idOnly[0].detail, 'Codex · gpt-5');
});


test('Gemini CLI uses its own client/model identity and Home shows the two most constrained model quotas', () => {
  assert.equal(modelVendorFor('gemini-3-pro'), 'gemini');
  const [gemini] = quotaRows({ providers: [{
    provider: 'gemini', planLabel: 'Google AI Pro', status: 'ok',
    windows: [
      { kind: 'other', additional: true, label: 'gemini-3-pro', remainingPercent: 65 },
      { kind: 'other', additional: true, label: 'gemini-3-flash', remainingPercent: 18 },
      { kind: 'other', additional: true, label: 'gemini-2.5-flash', remainingPercent: 42 },
    ],
  }] }).filter((row) => row.providerId === 'gemini');
  assert.equal(gemini.name, 'Gemini CLI');
  assert.equal(gemini.iconClass, 'row-icon-gemini');
  const [geminiTool] = toolRows({ totalTokens: 100, clients: { gemini: 100 } });
  assert.equal(geminiTool.name, 'Gemini CLI');
  assert.equal(geminiTool.iconClass, 'row-icon-gemini');
  assert.deepEqual(homeQuotaWindows(gemini).map((window) => window.label), [
    'gemini-3-flash',
    'gemini-2.5-flash',
  ]);
});


test('Antigravity keeps grouped quota labels and Home chooses the constrained lane per cadence', () => {
  const [antigravity] = quotaRows({ providers: [{
    provider: 'antigravity', planLabel: 'Pro', status: 'ok',
    windows: [
      { kind: 'session', label: 'Gemini 5-hour', remainingPercent: 72, source: 'antigravity-local' },
      { kind: 'weekly', label: 'Gemini Weekly', remainingPercent: 44, source: 'antigravity-local' },
      { kind: 'session', label: 'Claude + GPT 5-hour', remainingPercent: 31, source: 'antigravity-local' },
      { kind: 'weekly', label: 'Claude + GPT Weekly', remainingPercent: 83, source: 'antigravity-local' },
    ],
  }] }).filter((row) => row.providerId === 'antigravity');
  assert.equal(quotaWindowLabel(antigravity.windows[0]), 'Gemini 5-hour');
  assert.equal(quotaWindowLabel(antigravity.windows[2]), 'Claude + GPT 5-hour');
  assert.deepEqual(homeQuotaWindows(antigravity).map((window) => window.label), [
    'Claude + GPT 5-hour',
    'Gemini Weekly',
  ]);

  const legacy = { ...antigravity, windows: [
    { kind: 'other', label: 'Gemini', remainingPercent: 70 },
    { kind: 'other', label: 'Claude + GPT', remainingPercent: 20 },
  ] };
  assert.deepEqual(homeQuotaWindows(legacy).map((window) => window.label), ['Claude + GPT', 'Gemini']);
});
