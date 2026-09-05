import './electron/renderer/styles.css';
import './styles.css';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { installTokenMonitorFacade } from './token-monitor-facade.js';
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
const PERIODS = ['today', 'month', 'allTime'];
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
  period: 'today',
  view: 'home',
  refreshing: false,
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
          <button class="tab active" data-period="today">DAY</button>
          <button class="tab" data-period="month">MONTH</button>
          <button class="tab" data-period="allTime">TOTAL</button>
        </nav>
        <div class="actions-hotspot" aria-hidden="true"></div>
        <div class="window-actions">
          <button id="pinButton" class="icon-button" title="Cycle window behavior">⇧</button>
          <button id="minButton" class="icon-button" title="Minimize">−</button>
          <button id="closeButton" class="icon-button" title="Close">×</button>
        </div>
      </div>
    </header>
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
  homePanel: document.querySelector('#homePanel'),
  breakdown: document.querySelector('#breakdown'),
  limitsPanel: document.querySelector('#limitsPanel'),
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

function currentPeriod() {
  return state.stats?.periods?.[state.period] || { totalTokens: 0, costUsd: 0 };
}

function renderHeadline() {
  const period = currentPeriod();
  els.totalTokens.textContent = formatNumber(period.totalTokens);
  els.cost.textContent = formatCost(period.costUsd);
  for (const button of document.querySelectorAll('[data-period]')) {
    button.classList.toggle('active', button.dataset.period === state.period);
  }
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
  els.shell.classList.toggle('home-mode', state.view === 'home');
  els.shell.classList.toggle('session-mode', state.view === 'session');
  els.homePanel.classList.toggle('hidden', state.view !== 'home');
  els.limitsPanel.classList.toggle('hidden', state.view !== 'limits');
  els.breakdown.classList.toggle('hidden', !['tool', 'model', 'session'].includes(state.view));
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

async function refresh() {
  if (state.refreshing) return;
  state.refreshing = true;
  els.refreshButton.classList.add('is-refreshing');
  setStatus('Refreshing…');
  try {
    state.stats = await window.tokenMonitor.getStats();
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
  }
}

for (const button of document.querySelectorAll('[data-period]')) {
  button.addEventListener('click', () => {
    if (!PERIODS.includes(button.dataset.period)) return;
    state.period = button.dataset.period;
    render();
  });
}

els.refreshButton.addEventListener('click', refresh);
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

els.pinButton.classList.add('active');
renderViewSwitcher();
void refresh();
