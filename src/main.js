import './electron/renderer/styles.css';
import './styles.css';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { installTokenMonitorFacade } from './token-monitor-facade.js';
import {
  derivedRequest,
  displayLabel as periodDisplayLabel,
  normalizeMonthMode,
  periodMenuTargetIndex,
  slotForSelection,
} from './fixed-periods.js';
import {
  clientColor,
  formatCompact,
  formatCost,
  formatNumber,
  formatPercent,
  formatResetTime,
  modelRows,
  quotaRows,
  quotaWindowLabel,
  sessionRows,
  toolRows,
} from './renderer-model.js';

installTokenMonitorFacade();

const appWindow = getCurrentWindow();
const root = document.querySelector('#app');
const platformHint = `${navigator.platform || ''} ${navigator.userAgent || ''}`.toLowerCase();
const isWindows = platformHint.includes('win');
document.documentElement.classList.toggle('is-windows', isWindows);
document.body.classList.toggle('is-windows', isWindows);
const PERIODS = ['today', 'month', 'week', 'last7', 'last30', 'allTime'];
const MONTH_PERIODS = ['month', 'week', 'last7', 'last30'];
const AUTO_REFRESH_MS = 30 * 1000;
const VIEW_ORDER = ['home', 'tool', 'model', 'session', 'limits'];
const VIEW_META = Object.freeze({
  home: { label: 'Home', icon: 'view-icon-home' },
  tool: { label: 'Tools', icon: 'view-icon-tool' },
  model: { label: 'Models', icon: 'view-icon-model' },
  session: { label: 'Sessions', icon: 'view-icon-session' },
  limits: { label: 'Limits', icon: 'view-icon-limits' },
});

const state = {
  stats: null,
  settings: {
    floatingBubbleEnabled: false,
    floatingBubbleTrigger: 'click',
    floatingBubbleContent: 'icon',
  },
  floatingBubble: { collapsed: false, side: null },
  settingsOpen: false,
  period: 'today',
  monthMode: 'month',
  periodMenuOpen: false,
  view: 'home',
  refreshing: false,
  refreshQueued: false,
  refreshQueuedForce: false,
  lastRefreshAt: 0,
  viewMenuOpen: false,
  alwaysOnTop: true,
};

root.innerHTML = `
  <main class="shell home-mode" id="shell">
    <header class="titlebar" id="titlebar" data-tauri-drag-region>
      <div data-tauri-drag-region>
        <div class="app-title" data-tauri-drag-region>
          <span class="app-title-text">Token Lens</span><span class="app-title-mark">Σ</span><span id="liveDot" class="live-dot"></span>
        </div>
        <div id="status" class="status"></div>
      </div>
      <div class="title-controls">
        <nav class="tabs" aria-label="Period tabs">
          <span class="tab-indicator" aria-hidden="true"></span>
          <button class="tab active" data-period="today" data-period-slot="today">DAY</button>
          <button id="monthPeriodTab" class="tab" data-period-slot="month" aria-haspopup="menu" aria-expanded="false">MONTH</button>
          <button class="tab" data-period="allTime" data-period-slot="allTime">TOTAL</button>
        </nav>
        <div id="monthPeriodMenu" class="view-switcher-menu period-menu hidden" role="menu" aria-labelledby="monthPeriodTab">
          <button class="view-switcher-menu-item" type="button" data-fixed-period="month">This month</button>
          <button class="view-switcher-menu-item" type="button" data-fixed-period="week">This week</button>
          <button class="view-switcher-menu-item" type="button" data-fixed-period="last7">Last 7 days</button>
          <button class="view-switcher-menu-item" type="button" data-fixed-period="last30">Last 30 days</button>
        </div>
        <div class="actions-hotspot" aria-hidden="true"></div>
        <div class="window-actions">
          <button id="pinButton" class="icon-button" title="Cycle window behavior">⇧</button>
          <button id="minButton" class="icon-button" title="Minimize">−</button>
          <button id="closeButton" class="icon-button" title="Close">×</button>
        </div>
      </div>
    </header>
    <section id="settingsPanel" class="settings-panel hidden" aria-hidden="true">
      <div class="settings-group settings-collapsible-group v2-settings-card">
        <div class="settings-group-header"><span>Floating & Tray</span></div>
        <label class="checkbox-label settings-item">
          <span class="settings-item-text"><span class="settings-item-title">Floating Bubble</span></span>
          <input id="floatingBubbleInput" type="checkbox" />
          <span class="settings-note settings-item-desc">Collapse the widget into a draggable mini-window when it loses focus.</span>
        </label>
        <div id="floatingBubbleOptions" class="presence-feature-body hidden">
          <div class="settings-item">
            <span id="floatingBubbleTriggerLabel" class="settings-item-text"><span class="settings-item-title">Open on</span></span>
            <div class="inline-options" role="radiogroup" aria-labelledby="floatingBubbleTriggerLabel">
              <label class="inline-option"><input type="radio" name="floatingBubbleTrigger" value="click" /><span>Click</span></label>
              <label class="inline-option"><input type="radio" name="floatingBubbleTrigger" value="hover" /><span>Hover</span></label>
            </div>
          </div>
          <p class="settings-note v2-settings-note">Bubble display currently preserves the original icon-only mode.</p>
        </div>
      </div>
    </section>
    <section class="total-panel">
      <div class="label-row"><span>TOTAL TOKENS</span></div>
      <div class="total-number-row"><div id="totalTokens" class="total-number">0</div></div>
      <div id="cost" class="cost">$0.00</div>
    </section>
    <section id="homePanel" class="home-panel"></section>
    <section id="breakdown" class="breakdown hidden"></section>
    <section id="limitsPanel" class="limits-panel hidden"></section>
    <footer class="footer">
      <div id="viewSwitcher" class="view-switcher"></div>
      <span id="footerActionSlot">
        <span class="utility-actions is-swapped">
          <button id="refreshButton" class="refresh-button" title="Refresh" aria-label="Refresh"><span class="refresh-button-icon" aria-hidden="true">↻</span><span class="refresh-button-spinner" aria-hidden="true"></span></button>
          <button id="settingsButton" class="icon-button settings-icon-button" title="Settings" aria-label="Settings"></button>
        </span>
      </span>
    </footer>
  </main>
  <button id="floatingBubbleTab" class="floating-bubble-tab" type="button" aria-hidden="true"><span>Σ</span></button>
`;

const els = {
  shell: document.querySelector('#shell'),
  status: document.querySelector('#status'),
  liveDot: document.querySelector('#liveDot'),
  totalTokens: document.querySelector('#totalTokens'),
  cost: document.querySelector('#cost'),
  monthPeriodTab: document.querySelector('#monthPeriodTab'),
  monthPeriodMenu: document.querySelector('#monthPeriodMenu'),
  homePanel: document.querySelector('#homePanel'),
  breakdown: document.querySelector('#breakdown'),
  limitsPanel: document.querySelector('#limitsPanel'),
  settingsPanel: document.querySelector('#settingsPanel'),
  settingsButton: document.querySelector('#settingsButton'),
  floatingBubbleInput: document.querySelector('#floatingBubbleInput'),
  floatingBubbleOptions: document.querySelector('#floatingBubbleOptions'),
  floatingBubbleTriggerInputs: Array.from(document.querySelectorAll('input[name="floatingBubbleTrigger"]')),
  floatingBubbleTab: document.querySelector('#floatingBubbleTab'),
  viewSwitcher: document.querySelector('#viewSwitcher'),
  refreshButton: document.querySelector('#refreshButton'),
  pinButton: document.querySelector('#pinButton'),
  minButton: document.querySelector('#minButton'),
  closeButton: document.querySelector('#closeButton'),
};
function setStatus(message = '', error = false) {
  els.status.textContent = message;
  els.status.classList.toggle('error', Boolean(error && message));
  els.liveDot.classList.toggle('live', !error && Boolean(state.stats));
}

function applySettings(settings = {}) {
  state.settings = {
    ...state.settings,
    ...settings,
    floatingBubbleEnabled: settings.floatingBubbleEnabled === true,
    floatingBubbleTrigger: settings.floatingBubbleTrigger === 'hover' ? 'hover' : 'click',
    floatingBubbleContent: 'icon',
  };
  els.floatingBubbleInput.checked = state.settings.floatingBubbleEnabled;
  els.floatingBubbleOptions.classList.toggle('hidden', !state.settings.floatingBubbleEnabled);
  for (const input of els.floatingBubbleTriggerInputs) {
    input.checked = input.value === state.settings.floatingBubbleTrigger;
  }
}

function applyFloatingBubbleState(payload = {}) {
  const side = payload.collapsed && ['left', 'right'].includes(payload.side) ? payload.side : null;
  state.floatingBubble = { collapsed: Boolean(side), side };
  for (const node of [document.documentElement, document.body]) {
    node.classList.toggle('floating-bubble-collapsed-left', side === 'left');
    node.classList.toggle('floating-bubble-collapsed-right', side === 'right');
  }
  els.floatingBubbleTab.setAttribute('aria-hidden', String(!side));
  els.floatingBubbleTab.title = side ? 'Expand Token Lens' : '';
  if (side) {
    state.settingsOpen = false;
    els.shell.classList.remove('settings-open');
    els.settingsPanel.classList.add('hidden');
    els.settingsPanel.setAttribute('aria-hidden', 'true');
  }
}

async function saveSettings(patch) {
  const settings = await window.tokenMonitor.updateSettings(patch);
  applySettings(settings);
  const bubble = await window.tokenMonitor.getFloatingBubbleState();
  applyFloatingBubbleState(bubble);
  renderSurface();
  return settings;
}

function currentPeriod() {
  return state.stats?.periods?.[state.period] || { totalTokens: 0, costUsd: 0 };
}

function periodMenuButtons() {
  return Array.from(els.monthPeriodMenu?.querySelectorAll('[data-fixed-period]') || []);
}

function focusPeriodMenuButton(index) {
  const buttons = periodMenuButtons();
  if (!buttons.length) return;
  const target = buttons[Math.max(0, Math.min(buttons.length - 1, Number(index) || 0))];
  for (const button of buttons) button.tabIndex = button === target ? 0 : -1;
  target?.focus();
}

function setPeriodMenuOpen(open, { restoreFocus = false, focus = '' } = {}) {
  state.periodMenuOpen = Boolean(open);
  els.monthPeriodMenu?.closest('.titlebar')?.classList.toggle('period-menu-open', state.periodMenuOpen);
  els.monthPeriodMenu?.classList.toggle('hidden', !state.periodMenuOpen);
  els.monthPeriodTab?.setAttribute('aria-expanded', String(state.periodMenuOpen));
  syncPeriodMenu();
  if (state.periodMenuOpen && focus) {
    const buttons = periodMenuButtons();
    const current = Math.max(0, buttons.findIndex((button) => button.classList.contains('is-current')));
    focusPeriodMenuButton(focus === 'first' ? 0 : focus === 'last' ? buttons.length - 1 : current);
  }
  if (!state.periodMenuOpen && restoreFocus) els.monthPeriodTab?.focus();
}

function syncPeriodMenu() {
  const activeMode = slotForSelection(state.period) === 'month'
    ? normalizeMonthMode(state.period)
    : normalizeMonthMode(state.monthMode);
  els.settingsButton.addEventListener('click', () => {
  state.settingsOpen = !state.settingsOpen;
  state.viewMenuOpen = false;
  setPeriodMenuOpen(false);
  renderSurface();
});

els.floatingBubbleInput.addEventListener('change', () => {
  void saveSettings({ floatingBubbleEnabled: els.floatingBubbleInput.checked });
});
for (const input of els.floatingBubbleTriggerInputs) {
  input.addEventListener('change', () => {
    if (input.checked) void saveSettings({ floatingBubbleTrigger: input.value });
  });
}

els.floatingBubbleTab.addEventListener('pointerdown', (event) => {
  if (!state.floatingBubble.collapsed || event.button !== 0) return;
  clearBubbleTimer('hoverReveal');
  floatingBubbleDrag = {
    pointerId: event.pointerId,
    startX: event.screenX,
    startY: event.screenY,
    moved: false,
    ...floatingBubblePointerRatio(event),
  };
  els.floatingBubbleTab.setPointerCapture(event.pointerId);
  event.preventDefault();
});

els.floatingBubbleTab.addEventListener('pointermove', (event) => {
  const drag = floatingBubbleDrag;
  if (!drag || drag.pointerId !== event.pointerId) return;
  if (!drag.moved && Math.hypot(event.screenX - drag.startX, event.screenY - drag.startY) < 4) return;
  drag.moved = true;
  els.floatingBubbleTab.classList.add('dragging');
  void window.tokenMonitor.moveFloatingBubble({
    offsetRatioX: drag.offsetRatioX,
    offsetRatioY: drag.offsetRatioY,
  }).then(applyFloatingBubbleState).catch(console.error);
  event.preventDefault();
});

els.floatingBubbleTab.addEventListener('pointerup', (event) => {
  const drag = finishFloatingBubbleDrag(event.pointerId);
  if (!drag) return;
  if (drag.moved) {
    void window.tokenMonitor.moveFloatingBubble({
      offsetRatioX: drag.offsetRatioX,
      offsetRatioY: drag.offsetRatioY,
    }).then(applyFloatingBubbleState).catch(console.error);
  } else {
    void expandFloatingBubble({ focus: true });
  }
  event.preventDefault();
});
els.floatingBubbleTab.addEventListener('pointercancel', (event) => finishFloatingBubbleDrag(event.pointerId));
els.floatingBubbleTab.addEventListener('keydown', (event) => {
  if (!['Enter', ' '].includes(event.key)) return;
  event.preventDefault();
  void expandFloatingBubble({ focus: true });
});
els.floatingBubbleTab.addEventListener('mouseenter', () => {
  if (state.settings.floatingBubbleTrigger !== 'hover' || !state.floatingBubble.collapsed) return;
  clearBubbleTimer('hoverReveal');
  floatingBubbleHoverRevealTimer = setTimeout(() => {
    floatingBubbleHoverRevealTimer = null;
    if (!floatingBubbleDrag && state.floatingBubble.collapsed) void expandFloatingBubble({ focus: false });
  }, 250);
});
els.floatingBubbleTab.addEventListener('mouseleave', () => clearBubbleTimer('hoverReveal'));
document.addEventListener('mouseleave', () => {
  if (state.settings.floatingBubbleTrigger !== 'hover' || state.floatingBubble.collapsed) return;
  clearBubbleTimer('hoverCollapse');
  floatingBubbleHoverCollapseTimer = setTimeout(() => {
    floatingBubbleHoverCollapseTimer = null;
    void collapseFloatingBubbleIfIdle();
  }, 200);
});

for (const button of periodMenuButtons()) {
    const active = button.dataset.fixedPeriod === activeMode;
    button.classList.toggle('is-current', active);
    button.setAttribute('aria-checked', String(active));
    if (active) button.setAttribute('aria-current', 'true');
    else button.removeAttribute('aria-current');
    button.tabIndex = state.periodMenuOpen && active ? 0 : -1;
  }
}

function syncPeriodTabs() {
  const activeSlot = slotForSelection(state.period);
  const tabs = Array.from(document.querySelectorAll('.tab'));
  const activeIndex = Math.max(0, tabs.findIndex((tab) => tab.dataset.periodSlot === activeSlot));
  document.querySelector('.tabs')?.style.setProperty('--period-index', String(activeIndex));
  for (const tab of tabs) {
    const active = tab.dataset.periodSlot === activeSlot;
    tab.classList.toggle('active', active);
    tab.setAttribute('aria-pressed', String(active));
  }
  const mode = activeSlot === 'month' ? normalizeMonthMode(state.period) : normalizeMonthMode(state.monthMode);
  els.monthPeriodTab.textContent = periodDisplayLabel(mode);
  syncPeriodMenu();
}

function renderHeadline() {
  const period = currentPeriod();
  els.totalTokens.textContent = formatNumber(period.totalTokens);
  els.cost.textContent = formatCost(period.costUsd);
  syncPeriodTabs();
}

function iconSpan(iconClass, color = '') {
  const mark = document.createElement('span');
  mark.className = `row-mark row-icon ${iconClass}`;
  if (color) mark.style.color = color;
  mark.setAttribute('aria-hidden', 'true');
  return mark;
}

function homeModule(title, view, iconClass) {
  const module = document.createElement('button');
  module.type = 'button';
  module.className = `home-module home-module-${view}`;
  module.addEventListener('click', () => setView(view));
  const head = document.createElement('div');
  head.className = 'home-module-head';
  const titleWrap = document.createElement('div');
  titleWrap.className = 'home-module-title-wrap';
  const label = document.createElement('span');
  label.className = 'home-module-label';
  label.textContent = title;
  const jump = document.createElement('span');
  jump.className = `home-module-jump ${iconClass}`;
  titleWrap.append(label);
  head.append(titleWrap, jump);
  const body = document.createElement('div');
  body.className = 'home-module-body';
  module.append(head, body);
  return { module, body };
}

function homeListRow(row, kind) {
  const item = document.createElement('div');
  item.className = `home-list-row home-${kind}-row`;
  const mark = iconSpan(row.iconClass, row.color);
  mark.classList.add('home-list-mark');
  const name = document.createElement('span');
  name.className = 'home-list-name';
  name.textContent = row.name;
  const value = document.createElement('span');
  value.className = 'home-list-value';
  value.textContent = formatCompact(row.value);
  const share = document.createElement('span');
  share.className = 'home-list-aux';
  share.textContent = formatPercent(row.share * 100);
  item.append(mark, name, value, share);
  return item;
}

function renderHomeModels() {
  const { module, body } = homeModule('MODELS', 'model', 'view-icon-model');
  const rows = modelRows(currentPeriod()).slice(0, 5);
  if (!rows.length) {
    const empty = document.createElement('div');
    empty.className = 'home-module-empty';
    empty.textContent = 'No model usage';
    body.append(empty);
    return module;
  }
  for (const row of rows) body.append(homeListRow(row, 'model'));
  return module;
}

function renderHomeLimits() {
  const { module, body } = homeModule('LIMITS', 'limits', 'view-icon-limits');
  const rows = quotaRows(state.stats?.limits);
  for (const row of rows) {
    const account = document.createElement('div');
    account.className = 'home-limit-account';
    const head = document.createElement('div');
    head.className = 'home-limit-account-head';
    const mark = iconSpan(row.iconClass, row.color);
    mark.classList.add('home-list-mark');
    const name = document.createElement('span');
    name.className = 'home-list-name';
    name.textContent = row.plan ? `${row.name} · ${row.plan}` : row.name;
    head.append(mark, name);
    const windows = document.createElement('div');
    windows.className = 'home-limit-windows';
    const compactWindows = row.windows.filter((window) => !window.additional && ['session', 'weekly'].includes(window.kind)).slice(0, 2);
    if (!compactWindows.length) {
      const empty = document.createElement('div');
      empty.className = 'home-module-empty';
      empty.textContent = row.status === 'ok' ? 'No quota windows' : 'Unavailable';
      windows.append(empty);
    }
    for (const window of compactWindows) {
      const item = document.createElement('div');
      item.className = 'home-limit-window';
      const line = document.createElement('div');
      line.className = 'home-limit-window-line';
      const label = document.createElement('span');
      label.className = 'home-limit-window-label';
      label.textContent = quotaWindowLabel(window);
      const value = document.createElement('span');
      value.className = 'home-list-value';
      value.textContent = window.remainingPercent == null ? '—' : formatPercent(window.remainingPercent);
      if (window.remainingPercent != null && window.remainingPercent < 20) value.classList.add('home-limit-value-critical');
      else if (window.remainingPercent != null && window.remainingPercent < 50) {
        value.classList.add('home-limit-value-low');
        value.style.setProperty('--home-limit-accent', row.color);
      }
      line.append(label, value);
      item.append(line);
      const reset = formatResetTime(window.resetsAt);
      if (reset) {
        const resetText = document.createElement('span');
        resetText.className = 'home-limit-reset';
        resetText.textContent = reset;
        item.append(resetText);
      }
      windows.append(item);
    }
    account.append(head, windows);
    body.append(account);
  }
  return module;
}

function renderHome() {
  els.homePanel.replaceChildren(renderHomeLimits(), renderHomeModels());
}
function breakdownRow(row, max, kind) {
  const item = document.createElement('div');
  item.className = `row${kind === 'session' ? ' session-row' : ''}`;
  item.dataset.key = row.key;
  const head = document.createElement('div');
  head.className = 'row-head';
  const name = document.createElement('div');
  name.className = 'row-name';
  name.append(iconSpan(row.iconClass, row.color));
  const label = document.createElement('div');
  label.className = 'row-label';
  const title = document.createElement('span');
  title.className = 'row-title';
  title.textContent = row.name;
  label.append(title);
  if (row.detail) {
    const detail = document.createElement('span');
    detail.className = 'row-detail';
    detail.textContent = row.detail;
    label.append(detail);
  }
  name.append(label);
  const metrics = document.createElement('div');
  metrics.className = 'row-metrics';
  const value = document.createElement('span');
  value.className = 'row-value';
  value.textContent = formatNumber(row.value);
  const cost = document.createElement('span');
  cost.className = 'row-cost';
  cost.textContent = formatCost(row.cost);
  metrics.append(value, cost);
  head.append(name, metrics);
  const bar = document.createElement('div');
  bar.className = 'bar';
  const fill = document.createElement('div');
  fill.className = 'bar-fill';
  fill.style.background = row.color;
  fill.style.setProperty('--bar-scale', String(max > 0 ? Math.max(0, Math.min(1, row.value / max)) : 0));
  bar.append(fill);
  item.append(head, bar);
  return item;
}

function rowsForView() {
  const period = currentPeriod();
  if (state.view === 'tool') return toolRows(period);
  if (state.view === 'model') return modelRows(period);
  if (state.view === 'session') return sessionRows(period);
  return [];
}
function renderBreakdown() {
  const rows = rowsForView();
  const max = Math.max(1, ...rows.map((row) => row.value));
  if (!rows.length) {
    const empty = document.createElement('div');
    empty.className = 'home-module-empty';
    empty.textContent = state.view === 'session' ? 'No session usage' : 'No usage';
    els.breakdown.replaceChildren(empty);
    return;
  }
  els.breakdown.replaceChildren(...rows.map((row) => breakdownRow(row, max, state.view)));
}

function limitWindowNode(window, row) {
  const item = document.createElement('div');
  item.className = 'limit-window';
  if (window.additional) item.classList.add('limit-window-wide');
  const text = document.createElement('div');
  text.className = 'limit-window-text';
  const label = document.createElement('span');
  label.textContent = quotaWindowLabel(window);
  const value = document.createElement('span');
  value.textContent = window.remainingPercent == null ? '—' : `${formatPercent(window.remainingPercent)} left`;
  text.append(label, value);
  item.append(text);
  if (window.remainingPercent != null) {
    const meter = document.createElement('div');
    meter.className = 'limit-meter';
    meter.style.background = `${row.color}29`;
    const fill = document.createElement('div');
    fill.className = 'limit-meter-fill';
    fill.style.background = row.color;
    fill.style.setProperty('--bar-scale', String(Math.max(0, Math.min(1, window.remainingPercent / 100))));
    meter.append(fill);
    item.append(meter);
  }
  const reset = document.createElement('div');
  reset.className = 'limit-reset';
  reset.textContent = formatResetTime(window.resetsAt);
  if (reset.textContent) item.append(reset);
  return item;
}

function renderLimits() {
  const nodes = quotaRows(state.stats?.limits).map((row) => {
    const item = document.createElement('div');
    item.className = 'limit-row';
    const head = document.createElement('div');
    head.className = 'limit-head';
    const name = document.createElement('div');
    name.className = 'limit-name';
    name.append(iconSpan(row.iconClass, row.color));
    const title = document.createElement('span');
    title.textContent = row.name;
    name.append(title);
    const plan = document.createElement('span');
    plan.className = 'limit-plan';
    plan.textContent = row.plan || (row.status === 'ok' ? '' : 'Unavailable');
    head.append(name, plan);
    const windows = document.createElement('div');
    windows.className = 'limit-windows';
    for (const window of row.windows) windows.append(limitWindowNode(window, row));
    if (!row.windows.length) {
      const empty = document.createElement('div');
      empty.className = 'home-module-empty';
      empty.textContent = row.status === 'ok' ? 'No quota windows' : 'Unavailable';
      windows.append(empty);
    }
    if (row.providerId === 'codex' && row.resetCredits?.availableCount > 0) {
      const credits = document.createElement('div');
      credits.className = 'limit-window limit-window-note limit-window-wide';
      credits.innerHTML = `<div class="limit-window-text"><span>Rate-limit reset</span><span>${formatNumber(row.resetCredits.availableCount)} available</span></div>`;
      windows.append(credits);
    }
    item.append(head, windows);
    return item;
  });
  els.limitsPanel.replaceChildren(...nodes);
}
function setView(view) {
  if (!VIEW_ORDER.includes(view)) return;
  state.view = view;
  state.viewMenuOpen = false;
  render();
}

function statsRequestOptions(force = false, period = state.period) {
  const derived = derivedRequest(period, { locale: navigator.language });
  return { force, ...(derived ? { derived } : {}) };
}

function setPeriod(period) {
  if (!PERIODS.includes(period)) return false;
  const changed = state.period !== period;
  state.period = period;
  if (MONTH_PERIODS.includes(period)) state.monthMode = period;
  setPeriodMenuOpen(false);
  render();
  if (derivedRequest(period, { locale: navigator.language })) void refresh();
  return changed;
}

function renderViewSwitcher() {
  const meta = VIEW_META[state.view];
  const switcher = document.createElement('div');
  switcher.className = `view-switcher${state.viewMenuOpen ? ' is-open' : ''}`;
  const current = document.createElement('button');
  current.type = 'button';
  current.className = 'view-switcher-current';
  const icon = document.createElement('span');
  icon.className = `view-switcher-icon ${meta.icon}`;
  const label = document.createElement('span');
  label.className = 'view-switcher-label';
  label.textContent = meta.label;
  current.append(icon, label);
  current.addEventListener('click', () => {
    const index = VIEW_ORDER.indexOf(state.view);
    setView(VIEW_ORDER[(index + 1) % VIEW_ORDER.length]);
  });
  const disclosure = document.createElement('button');
  disclosure.type = 'button';
  disclosure.className = 'view-switcher-disclosure';
  disclosure.setAttribute('aria-label', 'Choose view');
  disclosure.setAttribute('aria-expanded', String(state.viewMenuOpen));
  disclosure.addEventListener('click', () => {
    state.viewMenuOpen = !state.viewMenuOpen;
    renderViewSwitcher();
  });
  const menu = document.createElement('div');
  menu.className = `view-switcher-menu${state.viewMenuOpen ? '' : ' hidden'}`;
  for (const view of VIEW_ORDER) {
    const item = document.createElement('button');
    item.type = 'button';
    item.className = `view-switcher-menu-item${view === state.view ? ' is-current' : ''}`;
    const itemIcon = document.createElement('span');
    itemIcon.className = `view-switcher-icon ${VIEW_META[view].icon}`;
    const itemLabel = document.createElement('span');
    itemLabel.className = 'view-switcher-menu-label';
    itemLabel.textContent = VIEW_META[view].label;
    item.append(itemIcon, itemLabel);
    item.addEventListener('click', () => setView(view));
    menu.append(item);
  }
  switcher.append(current, disclosure, menu);
  els.viewSwitcher.replaceChildren(...switcher.childNodes);
  els.viewSwitcher.className = switcher.className;
}

function renderSurface() {
  const settingsOpen = state.settingsOpen;
  els.shell.classList.toggle('settings-open', settingsOpen);
  els.shell.classList.toggle('home-mode', !settingsOpen && state.view === 'home');
  els.shell.classList.toggle('session-mode', !settingsOpen && state.view === 'session');
  els.settingsPanel.classList.toggle('hidden', !settingsOpen);
  els.settingsPanel.setAttribute('aria-hidden', String(!settingsOpen));
  els.homePanel.classList.toggle('hidden', settingsOpen || state.view !== 'home');
  els.limitsPanel.classList.toggle('hidden', settingsOpen || state.view !== 'limits');
  els.breakdown.classList.toggle('hidden', settingsOpen || !['tool', 'model', 'session'].includes(state.view));
  if (settingsOpen) return;
  if (state.view === 'home') renderHome();
  else if (state.view === 'limits') renderLimits();
  else renderBreakdown();
}

function render() {
  if (!state.stats) return;
  renderHeadline();
  renderViewSwitcher();
  renderSurface();
}

async function refresh({ force = false } = {}) {
  if (state.refreshing) {
    state.refreshQueued = true;
    state.refreshQueuedForce ||= force;
    return;
  }
  state.refreshing = true;
  const requestPeriod = state.period;
  els.refreshButton.classList.add('is-refreshing');
  setStatus('Refreshing…');
  try {
    state.stats = await window.tokenMonitor.getStats(statsRequestOptions(force, requestPeriod));
    state.lastRefreshAt = Date.now();
    setStatus();
    render();
    els.liveDot.classList.add('pulse');
    setTimeout(() => els.liveDot.classList.remove('pulse'), 1200);
  } catch (error) {
    console.error(error);
    setStatus(error?.message || 'Failed to refresh usage', true);
  } finally {
    state.refreshing = false;
    els.refreshButton.classList.remove('is-refreshing');
    if (state.refreshQueued) {
      const queuedForce = state.refreshQueuedForce;
      state.refreshQueued = false;
      state.refreshQueuedForce = false;
      queueMicrotask(() => void refresh({ force: queuedForce }));
    }
  }
}

let floatingBubbleCollapseTimer = null;
let floatingBubbleHoverRevealTimer = null;
let floatingBubbleHoverCollapseTimer = null;
let floatingBubbleDrag = null;
let stopFocusListener = null;

function clearBubbleTimer(name) {
  const timer = name === 'collapse'
    ? floatingBubbleCollapseTimer
    : name === 'hoverReveal'
      ? floatingBubbleHoverRevealTimer
      : floatingBubbleHoverCollapseTimer;
  if (timer) clearTimeout(timer);
  if (name === 'collapse') floatingBubbleCollapseTimer = null;
  else if (name === 'hoverReveal') floatingBubbleHoverRevealTimer = null;
  else floatingBubbleHoverCollapseTimer = null;
}

async function collapseFloatingBubbleIfIdle() {
  if (!state.settings.floatingBubbleEnabled || state.floatingBubble.collapsed) return;
  try {
    applyFloatingBubbleState(await window.tokenMonitor.collapseFloatingBubbleIfIdle());
  } catch (error) {
    console.error(error);
  }
}

async function expandFloatingBubble({ focus = true } = {}) {
  if (!state.floatingBubble.collapsed) return;
  try {
    const next = focus
      ? await window.tokenMonitor.expandFloatingBubble()
      : await window.tokenMonitor.peekFloatingBubble();
    applyFloatingBubbleState(next);
    render();
  } catch (error) {
    console.error(error);
  }
}

function scheduleFloatingBubbleCollapse(delay = 180) {
  clearBubbleTimer('collapse');
  if (!state.settings.floatingBubbleEnabled || state.floatingBubble.collapsed) return;
  floatingBubbleCollapseTimer = setTimeout(() => {
    floatingBubbleCollapseTimer = null;
    void collapseFloatingBubbleIfIdle();
  }, delay);
}

function floatingBubblePointerRatio(event) {
  const rect = els.floatingBubbleTab.getBoundingClientRect();
  const width = rect.width || 34;
  const height = rect.height || 34;
  return {
    offsetRatioX: Math.max(0, Math.min(1, (event.clientX - rect.left) / width)),
    offsetRatioY: Math.max(0, Math.min(1, (event.clientY - rect.top) / height)),
  };
}

function finishFloatingBubbleDrag(pointerId) {
  if (!floatingBubbleDrag || floatingBubbleDrag.pointerId !== pointerId) return null;
  const drag = floatingBubbleDrag;
  floatingBubbleDrag = null;
  els.floatingBubbleTab.classList.remove('dragging');
  try { els.floatingBubbleTab.releasePointerCapture(pointerId); } catch (_) {}
  return drag;
}

for (const tab of document.querySelectorAll('.tab')) {
  tab.addEventListener('click', (event) => {
    const slot = tab.dataset.periodSlot || tab.dataset.period;
    const activeSlot = slotForSelection(state.period);
    if (slot === 'month' && activeSlot === 'month') {
      event.stopPropagation();
      setPeriodMenuOpen(!state.periodMenuOpen, { focus: state.periodMenuOpen ? '' : 'current' });
      return;
    }
    setPeriodMenuOpen(false);
    setPeriod(slot === 'month' ? normalizeMonthMode(state.monthMode) : tab.dataset.period);
  });
}

els.refreshButton.addEventListener('click', () => void refresh({ force: true }));
els.minButton.addEventListener('click', () => appWindow.minimize());
els.closeButton.addEventListener('click', () => appWindow.close());
els.pinButton.addEventListener('click', async () => {
  state.alwaysOnTop = !state.alwaysOnTop;
  try {
    await appWindow.setAlwaysOnTop(state.alwaysOnTop);
    els.pinButton.classList.toggle('active', state.alwaysOnTop);
    els.pinButton.title = state.alwaysOnTop ? 'Floating above apps' : 'Normal window';
  } catch (error) {
    console.error(error);
  }
});

for (const button of periodMenuButtons()) {
  button.addEventListener('click', (event) => {
    event.stopPropagation();
    const selection = normalizeMonthMode(button.dataset.fixedPeriod);
    state.monthMode = selection;
    setPeriodMenuOpen(false, { restoreFocus: true });
    setPeriod(selection);
  });
}

els.monthPeriodTab?.addEventListener('keydown', (event) => {
  if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;
  event.preventDefault();
  setPeriodMenuOpen(true, { focus: event.key === 'ArrowUp' ? 'last' : 'first' });
});

els.monthPeriodMenu?.addEventListener('keydown', (event) => {
  if (event.key === 'Tab') {
    setPeriodMenuOpen(false);
    return;
  }
  const buttons = periodMenuButtons();
  const currentIndex = buttons.findIndex((button) => button === event.target);
  const targetIndex = periodMenuTargetIndex(event.key, currentIndex, buttons.length);
  if (targetIndex < 0) return;
  event.preventDefault();
  focusPeriodMenuButton(targetIndex);
});

document.addEventListener('click', (event) => {
  if (!state.periodMenuOpen) return;
  if (els.monthPeriodMenu?.contains(event.target) || els.monthPeriodTab?.contains(event.target)) return;
  setPeriodMenuOpen(false);
});

document.addEventListener('keydown', (event) => {
  if (event.key !== 'Escape') return;
  if (state.periodMenuOpen) {
    event.preventDefault();
    setPeriodMenuOpen(false, { restoreFocus: true });
    return;
  }
  if (state.settingsOpen) {
    event.preventDefault();
    state.settingsOpen = false;
    renderSurface();
    els.settingsButton.focus();
  }
});

const autoRefreshTimer = setInterval(() => {
  if (document.visibilityState === 'visible') void refresh();
}, AUTO_REFRESH_MS);

document.addEventListener('visibilitychange', () => {
  if (document.visibilityState !== 'visible') return;
  if (Date.now() - state.lastRefreshAt >= AUTO_REFRESH_MS) void refresh();
});
async function bootstrapShell() {
  try {
    const [settings, bubble] = await Promise.all([
      window.tokenMonitor.getSettings(),
      window.tokenMonitor.getFloatingBubbleState(),
    ]);
    applySettings(settings);
    applyFloatingBubbleState(bubble);
  } catch (error) {
    console.error(error);
    setStatus(error?.message || 'Failed to load settings', true);
  }

  stopFocusListener = await appWindow.onFocusChanged(({ payload: focused }) => {
    if (focused) clearBubbleTimer('collapse');
    else scheduleFloatingBubbleCollapse();
  });
  await refresh();
}

window.addEventListener('beforeunload', () => {
  clearInterval(autoRefreshTimer);
  clearBubbleTimer('collapse');
  clearBubbleTimer('hoverReveal');
  clearBubbleTimer('hoverCollapse');
  stopFocusListener?.();
}, { once: true });

els.pinButton.classList.add('active');
syncPeriodTabs();
renderViewSwitcher();
void bootstrapShell();
