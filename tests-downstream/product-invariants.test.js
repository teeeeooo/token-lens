'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');

const {
  fetchCodexLimits,
  mapCodexRateLimitsToProvider
} = require('../src/shared/limitCollector');
const homeOverview = require('../src/electron/renderer/homeOverview');
const { formatCompactMoney } = require('../src/shared/limitBalanceDisplay');
const {
  DEFAULT_INTERFACE_FONT,
  FONT_PRESETS,
  SYSTEM_UI_FONT
} = require('../src/shared/fontSettings');

const repoRoot = path.join(__dirname, '..');
const readRepo = (relativePath) => fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');

test('Codex Business individualLimit replaces a generic monthly lane with credit usage', () => {
  const provider = mapCodexRateLimitsToProvider({
    rateLimitsByLimitId: {
      codex: {
        planType: 'self_serve_business_prolite',
        primary: {
          usedPercent: 20,
          windowDurationMins: 5 * 60,
          resetsAt: '2026-09-04T08:00:00Z'
        },
        secondary: {
          usedPercent: 40,
          windowDurationMins: 30 * 24 * 60,
          resetsAt: '2026-10-01T00:00:00Z'
        },
        individualLimit: {
          limit: '25000',
          used: '8000',
          remainingPercent: 68,
          resetsAt: 1798761600
        }
      }
    }
  });

  const monthly = provider.windows.filter((window) => window.kind === 'billing' && window.additional !== true);
  assert.equal(monthly.length, 1);
  assert.equal(monthly[0].metric, 'credits');
  assert.equal(monthly[0].currency, 'CREDITS');
  assert.equal(monthly[0].label, 'Monthly');
  assert.equal(monthly[0].used, 8000);
  assert.equal(monthly[0].limit, 25000);
  assert.equal(monthly[0].remaining, 17000);
  assert.equal(monthly[0].usedPercent, 32);
  assert.equal(monthly[0].remainingPercent, 68);
  assert.ok(monthly[0].resetsAt);

  const fallback = mapCodexRateLimitsToProvider({
    rateLimitsByLimitId: {
      codex: {
        primary: {
          usedPercent: 20,
          windowDurationMins: 5 * 60,
          resetsAt: '2026-09-04T08:00:00Z'
        },
        secondary: {
          usedPercent: 40,
          windowDurationMins: 30 * 24 * 60,
          resetsAt: '2026-10-01T00:00:00Z'
        }
      }
    }
  });
  const fallbackMonthly = fallback.windows.filter((window) => window.kind === 'billing' && window.additional !== true);
  assert.equal(fallbackMonthly.length, 1);
  assert.equal(fallbackMonthly[0].metric, undefined);
  assert.equal(fallbackMonthly[0].usedPercent, 40);
});

test('Codex Business keeps OAuth quotas and supplements only individualLimit from App Server', async () => {
  let rpcCalls = 0;
  const provider = await fetchCodexLimits({}, {
    now: () => Date.parse('2026-09-04T04:00:00Z'),
    readCodexUsage: async () => ({
      rateLimitsByLimitId: {
        codex: {
          limitId: 'codex',
          planType: 'business',
          primary: {
            usedPercent: 15,
            windowDurationMins: 5 * 60,
            resetsAt: 1790000000
          }
        }
      }
    }),
    readCodexRpc: async () => {
      rpcCalls += 1;
      return {
        rateLimitsByLimitId: {
          codex: {
            limitId: 'codex',
            planType: 'business',
            individualLimit: {
              limit: '750',
              used: '432.762320022503',
              remainingPercent: 42,
              resetsAt: 1790812800
            }
          }
        }
      };
    },
    readCodexResetCredits: async () => null
  });

  assert.equal(rpcCalls, 1);
  assert.equal(provider.source, 'oauth');
  const session = provider.windows.find((window) => window.kind === 'session');
  assert.ok(session);
  assert.equal(session.usedPercent, 15);
  const monthly = provider.windows.find((window) => window.kind === 'billing' && window.additional !== true);
  assert.ok(monthly);
  assert.equal(monthly.metric, 'credits');
  assert.equal(monthly.currency, 'CREDITS');
  assert.equal(monthly.limit, 750);
  assert.equal(monthly.used, 432.762320022503);
  assert.equal(monthly.remainingPercent, 42);
  assert.equal(monthly.resetsAt, '2026-10-01T00:00:00.000Z');
});

test('Codex individualLimit enrichment is skipped for non-Business plans', async () => {
  let rpcCalls = 0;
  const provider = await fetchCodexLimits({}, {
    readCodexUsage: async () => ({
      rateLimitsByLimitId: {
        codex: {
          planType: 'pro',
          primary: {
            usedPercent: 25,
            windowDurationMins: 5 * 60,
            resetsAt: 1790000000
          }
        }
      }
    }),
    readCodexRpc: async () => {
      rpcCalls += 1;
      throw new Error('RPC should not be called for Pro');
    },
    readCodexResetCredits: async () => null
  });

  assert.equal(rpcCalls, 0);
  assert.equal(provider.status, 'ok');
  assert.equal(provider.windows.some((window) => window.kind === 'billing'), false);
});

test('Codex Business RPC enrichment failure preserves the healthy OAuth result', async () => {
  let rpcCalls = 0;
  const provider = await fetchCodexLimits({}, {
    readCodexUsage: async () => ({
      rateLimitsByLimitId: {
        codex: {
          planType: 'business',
          primary: {
            usedPercent: 30,
            windowDurationMins: 5 * 60,
            resetsAt: 1790000000
          }
        }
      }
    }),
    readCodexRpc: async () => {
      rpcCalls += 1;
      throw new Error('app-server temporarily unavailable');
    },
    readCodexResetCredits: async () => null
  });

  assert.equal(rpcCalls, 1);
  assert.equal(provider.source, 'oauth');
  assert.equal(provider.status, 'ok');
  assert.equal(provider.windows.find((window) => window.kind === 'session')?.usedPercent, 30);
  assert.equal(provider.windows.some((window) => window.kind === 'billing'), false);
});

test('Home keeps fixed credit quota metadata without provider-specific branching', () => {
  const [row] = homeOverview.homeLimitAccounts([{
    key: 'example',
    providerId: 'claude',
    windows: [{ kind: 'billing', metric: 'credits', currency: 'CREDITS', label: 'Monthly', used: 432.762320022503, limit: 750, remaining: 317.237679977497, usedPercent: 58 }]
  }], 3, { sort: 'configured' });
  assert.ok(row);
  const [monthly] = row.windows;
  assert.equal(monthly.currency, 'CREDITS');
  assert.equal(monthly.used, 432.762320022503);
  assert.equal(monthly.limit, 750);
  assert.equal(monthly.remaining, 317.237679977497);
  assert.equal(monthly.remainingPercent, 42);
});

test('CREDITS stays non-monetary while real currency formatting is unchanged', () => {
  assert.equal(formatCompactMoney(317.237679977497, 'CREDITS'), '317.24');
  assert.equal(formatCompactMoney(317.237679977497, 'USD'), '$317.24');
});

test('Home fixed-credit rendering is capability-based and downstream-materialized', () => {
  const app = readRepo('src/electron/renderer/app.js');
  const overview = readRepo('src/electron/renderer/homeOverview.js');
  const patcher = readRepo('scripts/downstream/apply-renderer-hardening.js');
  assert.match(overview, /used: finiteNumber\(window\.used\),\n\s+limit: finiteNumber\(window\.limit\)/);
  assert.match(app, /toUpperCase\(\) === 'CREDITS'/);
  assert.match(app, /formatLimitCount\(window, showUsed\)/);
  assert.match(app, /percentage \+ ' · ' \+ count \+ ' credits'/);
  assert.match(patcher, /patchUnifiedHomeQuotaPresentation/);
});

test('Token Lens defaults to the system UI font while preserving the mono preset', () => {
  assert.equal(DEFAULT_INTERFACE_FONT, SYSTEM_UI_FONT);
  assert.equal(FONT_PRESETS.system, SYSTEM_UI_FONT);
  assert.notEqual(FONT_PRESETS.mono, SYSTEM_UI_FONT);
  assert.match(FONT_PRESETS.mono, /ui-monospace/);
});

test('Token Lens promotes 10px and 11px UI text by one step without a new renderer', () => {
  const styles = readRepo('src/electron/renderer/styles.css');
  const dashboardStyles = readRepo('src/electron/renderer/dashboard.css');
  const app = readRepo('src/electron/renderer/app.js');

  assert.match(styles, /--token-lens-font-size-body:\s*12px;/);
  assert.match(styles, /--token-lens-font-size-small:\s*11px;/);
  assert.doesNotMatch(styles, /font-size:\s*(?:10|11)px;/);
  assert.doesNotMatch(dashboardStyles, /font-size:\s*(?:10|11)px;/);

  assert.match(app, /monthly\.metric === 'credits'/);
  assert.match(app, /formatLimitCount\(monthly, Boolean\(state\.settings\?\.showLimitUsed\)\)/);
  assert.match(app, /monthlyCount \? `\$\{monthlyCount\} credits` : ''/);
});