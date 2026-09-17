import { test as base, expect } from '@playwright/test';

const test = base.extend({
  page: async ({ page }, use) => {
    const errors = [];
    page.on('pageerror', (error) => errors.push(error.message));
    await use(page);
    expect(errors).toEqual([]);
  },
});

async function boot(page, config = {}) {
  await page.addInitScript((config) => {
    const fake = window.fake = { calls: [], pending: [], tokens: 10, rows: 1, quotaPercent: 50, ...config };
    fake.usage = (tokens = fake.tokens) => ({ generatedAtMs: Date.now(), source: 'synthetic', entries: Array.from({ length: fake.rows }, (_, i) => ({
      client: i % 2 ? 'claude' : 'codex', sessionId: `safe-${i}`, model: `model-${i}`, input: tokens, cost: 0.01,
    })) });
    fake.quota = (percent = fake.quotaPercent) => ({ generatedAtMs: Date.now(), providers: [{
      provider: 'codex', status: 'ready', recoveryState: 'idle', lastSuccessAtMs: Date.now(),
      windows: [{ kind: 'session', label: '5h', remainingPercent: percent, usedPercent: null, used: null, remaining: null, limit: null }],
    }] });
    window.__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: 'main' } }, transformCallback: () => 1,
      async invoke(command, args = {}) {
        fake.calls.push({ command, ...args });
        if (command === 'get_settings') return { language: 'en', floatingBubbleEnabled: true };
        if (command === 'get_floating_bubble_state' || command === 'set_floating_bubble_width') return { collapsed: false };
        if (command === 'get_usage_report') {
          if (args.period === 'today' && fake.failToday) throw new Error('synthetic usage failure');
          if (args.period === 'month' && fake.holdMonth) return new Promise((resolve, reject) => fake.pending.push({ kind: 'month', resolve, reject }));
          if (args.period === 'month' && fake.failMonth) throw new Error('synthetic month failure');
          return fake.usage();
        }
        if (command === 'get_quota_report') {
          if (fake.holdQuota) return new Promise((resolve, reject) => fake.pending.push({ kind: 'quota', resolve, reject }));
          if (fake.failQuota) throw new Error('synthetic quota failure');
          return fake.quota();
        }
        if (command === 'get_usage_since_report') {
          if (fake.failDerived) throw new Error('synthetic derived failure');
          return fake.usage();
        }
        if (command === 'get_session_metadata') {
          if (fake.holdMetadata) return new Promise((resolve, reject) => fake.pending.push({ kind: 'metadata', refs: args.sessions, resolve, reject }));
          return { sessions: args.sessions.map((ref) => ({ ...ref, sessionTitle: `Title ${ref.sessionId}` })) };
        }
        if (command === 'get_dashboard_history') {
          if (fake.holdHistory) return new Promise((resolve, reject) => fake.pending.push({ kind: 'history', resolve, reject }));
          return { generatedAtMs: Date.now(), daily: [], summary: { activeDays: fake.activeDays || 0 } };
        }
        return null;
      },
    };
  }, config);
  await page.goto('/');
}

async function view(page, name) {
  await page.locator('.view-switcher-disclosure').click();
  await page.locator('.view-switcher-menu-item').filter({ hasText: name }).click();
}

async function refresh(page) {
  await page.locator('.utility-actions').hover();
  await page.locator('#refreshButton').click();
}

async function settled(page) {
  await expect(page.locator('#refreshButton')).not.toHaveClass(/is-refreshing/);
}

test('delayed bootstrap shows Today and quota despite Month failure', async ({ page }) => {
  await boot(page, { holdMonth: true, holdQuota: true });
  await expect(page.locator('#totalTokens')).toHaveText('10');
  await expect.poll(() => page.evaluate(() => window.fake.pending.length)).toBe(2);
  await page.evaluate(() => {
    window.fake.pending.find((p) => p.kind === 'quota').resolve(window.fake.quota());
    window.fake.pending.find((p) => p.kind === 'month').reject(new Error('synthetic timeout'));
  });
  await expect(page.locator('.home-limit-window .home-list-value').first()).toHaveText('50%');
  await expect.poll(() => page.evaluate(() => window.fake.calls.some((c) => c.period === 'all_time'))).toBe(true);
  await page.locator('#monthPeriodTab').click();
  await expect(page.locator('#totalTokens')).toHaveText('—');
});

test('first Today failure remains unavailable while independent quota loads', async ({ page }) => {
  await boot(page, { failToday: true });
  await expect(page.locator('#totalTokens')).toHaveText('—');
  await expect(page.locator('.home-limit-window .home-list-value').first()).toHaveText('50%');
  expect(await page.evaluate(() => window.fake.calls.some((c) => c.command === 'update_tray_summary'))).toBe(false);
});

test('quota failure commits fresh usage and preserves a visibly stale quota', async ({ page }) => {
  await boot(page);
  await expect(page.locator('.home-limit-window .home-list-value').first()).toHaveText('50%');
  await page.evaluate(() => { window.fake.tokens = 20; window.fake.failQuota = true; });
  await refresh(page);
  await expect(page.locator('#totalTokens')).toHaveText('20');
  await settled(page);
  await expect(page.locator('#homePanel')).toContainText('Stale');
  await expect(page.locator('.home-limit-window .home-list-value').first()).toHaveText('50%');
});

test('forced refresh owns the DOM after older bootstrap results arrive', async ({ page }) => {
  await boot(page, { holdMonth: true, holdQuota: true });
  await expect.poll(() => page.evaluate(() => window.fake.pending.length)).toBe(2);
  await page.evaluate(() => { Object.assign(window.fake, { tokens: 20, quotaPercent: 80, holdMonth: false, holdQuota: false }); });
  await refresh(page);
  await expect(page.locator('#totalTokens')).toHaveText('20');
  await expect(page.locator('.home-limit-window .home-list-value').first()).toHaveText('80%');
  await settled(page);
  await page.evaluate(() => {
    for (const pending of window.fake.pending) pending.resolve(pending.kind === 'month' ? window.fake.usage(3) : window.fake.quota(3));
  });
  await page.locator('#monthPeriodTab').click();
  await expect(page.locator('#totalTokens')).toHaveText('20');
  await expect(page.locator('.home-limit-window .home-list-value').first()).toHaveText('80%');
});

test('null quota is unknown in the actual Limits DOM', async ({ page }) => {
  await boot(page, { quotaPercent: null });
  await view(page, 'Limits');
  await expect(page.locator('.limit-window-text').first()).toContainText('—');
  await expect(page.locator('.limit-meter')).toHaveCount(0);
  await expect(page.locator('#limitsPanel')).not.toContainText('0%');
});

test('filter overlay does not move the list and partial updates preserve filter and scroll', async ({ page }) => {
  await boot(page, { rows: 100 });
  await expect(page.locator('#totalTokens')).toHaveText('1,000');
  await view(page, 'Models');
  const list = page.locator('#breakdown');
  await list.evaluate((node) => { node.scrollTop = 400; });
  const before = await list.boundingBox();
  await page.locator('.provider-filter-button').click();
  await expect(page.locator('.provider-filter-menu')).toBeVisible();
  expect(await list.boundingBox()).toEqual(before);
  expect(await list.evaluate((node) => node.scrollTop)).toBe(400);
  await page.locator('.provider-filter-item').filter({ hasText: 'Codex' }).click();
  await list.evaluate((node) => { node.scrollTop = 350; });
  await refresh(page);
  await settled(page);
  await expect(page.locator('.provider-filter-button')).toContainText('Codex');
  expect(await list.evaluate((node) => node.scrollTop)).toBe(350);
  await expect(page.locator('#totalTokens')).toHaveText('500');
});

test('a failed new rolling range cannot display the previous date range as current', async ({ page }) => {
  await page.clock.setFixedTime(new Date('2026-09-18T12:00:00Z'));
  await boot(page);
  await page.locator('#monthPeriodTab').press('ArrowDown');
  await page.locator('[data-fixed-period="last7"]').click();
  await expect(page.locator('#totalTokens')).toHaveText('10');
  await settled(page);
  await page.clock.setFixedTime(new Date('2026-09-19T12:00:00Z'));
  await page.evaluate(() => { window.fake.failDerived = true; });
  await refresh(page);
  await settled(page);
  await expect(page.locator('#totalTokens')).toHaveText('—');
});

test('forced history refresh ignores an older in-flight history response', async ({ page }) => {
  await boot(page, { holdHistory: true });
  await expect.poll(() => page.evaluate(() => window.fake.pending.some((p) => p.kind === 'history'))).toBe(true);
  await page.evaluate(() => { window.fake.holdHistory = false; window.fake.activeDays = 7; });
  await refresh(page);
  await expect(page.locator('.v2-history-module .home-module-meta').first()).toContainText('7');
  await page.evaluate(() => {
    window.fake.pending.find((p) => p.kind === 'history').resolve({ generatedAtMs: 1, daily: [], summary: { activeDays: 1 } });
  });
  await settled(page);
  await expect(page.locator('.v2-history-module .home-module-meta').first()).toContainText('7');
});

test('session metadata arrival preserves the open filter overlay and list scroll', async ({ page }) => {
  await boot(page, { rows: 100, holdMetadata: true });
  await view(page, 'Sessions');
  await expect.poll(() => page.evaluate(() => window.fake.pending.some((p) => p.kind === 'metadata'))).toBe(true);
  const list = page.locator('#breakdown');
  await list.evaluate((node) => { node.scrollTop = 350; });
  await page.locator('.provider-filter-button').click();
  await page.evaluate(() => {
    const pending = window.fake.pending.find((p) => p.kind === 'metadata');
    pending.resolve({ sessions: pending.refs.map((ref) => ({ ...ref, sessionTitle: `Title ${ref.sessionId}` })) });
  });
  await settled(page);
  await expect(list.locator('.row-title').first()).toContainText('Title');
  await expect(page.locator('.provider-filter-menu')).toBeVisible();
  expect(await list.evaluate((node) => node.scrollTop)).toBe(350);
});
