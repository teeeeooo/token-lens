import test from 'node:test';
import assert from 'node:assert/strict';
import {
  formatCompact,
  modelRows,
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
test('quota rows keep the three-provider product surface and additional lanes', () => {
  const rows = quotaRows({ providers: [{
    provider: 'codex', planLabel: 'Plus', status: 'ok',
    windows: [
      { kind: 'session', remainingPercent: 35, label: '5h' },
      { kind: 'weekly', remainingPercent: 0, label: 'Weekly' },
      { kind: 'weekly', additional: true, remainingPercent: 88, label: 'GPT Reserve weekly' },
    ],
    resetCredits: { availableCount: 1 },
  }] });

  assert.deepEqual(rows.map((row) => row.providerId), ['codex', 'claude', 'antigravity']);
  assert.equal(rows[0].plan, 'Plus');
  assert.equal(rows[0].windows[2].additional, true);
  assert.equal(rows[0].resetCredits.availableCount, 1);
  assert.equal(rows[1].status, 'unavailable');
  assert.equal(rows[2].status, 'unavailable');
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
