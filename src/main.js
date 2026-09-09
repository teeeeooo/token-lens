import './electron/renderer/styles.css';
import './styles.css';
import { listen } from '@tauri-apps/api/event';
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
  clientLabel,
  compactTotalLabel,
  formatCompact,
  formatCost,
  formatNumber,
  formatPercent,
  homeQuotaRows,
  homeQuotaWindows,
  formatQuotaCount,
  formatResetTime,
  modelRows,
  quotaRows,
  quotaWindowLabel,
  quotaWindowSelectionKey,
  providerFilteredTotals,
  sessionRows,
  toolRows,
} from './renderer-model.js';
import { exchangeRows, periodStartTimeMs } from './session-detail-model.js';
import { historyViewModel } from './history-model.js';
import {
  bubblePercentLabel,
  floatingBubbleModel,
  normalizeBubbleContent,
  normalizeBubbleScale,
} from './floating-bubble-model.js';
import { THEME_PRESETS, applyThemePreset, normalizeThemePreset, normalizeZoomFactor } from './appearance-model.js';
import { applyTranslations, normalizeLanguage, resolveLanguage, translate } from './i18n.js';

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
const HISTORY_REFRESH_MS = 10 * 60 * 1000;
const APP_BOOTSTRAP_STARTED_AT = performance.now();
recordStartupTiming('renderer-bootstrap-start');
const BUBBLE_LOGICAL_HEIGHT = 34;
const BUBBLE_MAX_WIDTH = 240;
const VIEW_ORDER = ['home', 'tool', 'model', 'session', 'limits'];
const PROVIDER_FILTER_ORDER = ['codex', 'claude', 'gemini', 'antigravity'];
const VIEW_META = Object.freeze({
  home: { labelKey: 'view.home', icon: 'view-icon-home' },
  tool: { labelKey: 'view.tool', icon: 'view-icon-tool' },
  model: { labelKey: 'view.model', icon: 'view-icon-model' },
  session: { labelKey: 'view.session', icon: 'view-icon-session' },
  limits: { labelKey: 'view.limits', icon: 'view-icon-limits' },
});
let activeLanguage = resolveLanguage('auto', navigator.languages);
let activeLocale = navigator.languages?.[0] || navigator.language || 'en';
const t = (key, params = {}) => translate(activeLanguage, key, params);
const currentLocale = () => activeLocale;

function normalizeHomeQuotaSelections(value) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return {};
  const normalized = {};
  for (const [provider, rawKeys] of Object.entries(value)) {
    if (!Array.isArray(rawKeys)) continue;
    const keys = [];
    for (const rawKey of rawKeys) {
      const key = String(rawKey || '').trim().toLowerCase();
      if (key && !keys.includes(key)) keys.push(key);
      if (keys.length === 2) break;
    }
    normalized[String(provider || '').trim().toLowerCase()] = keys;
  }
  return normalized;
}

const state = {
  stats: null,
  history: null,
  historyLoading: false,
  historyError: '',
  historyLoadedAt: 0,
  historyScrollLeft: null,
  historyFollowEnd: true,
  settings: {
    showTrayIcon: true,
    floatingBubbleEnabled: true,
    floatingBubbleTrigger: 'click',
    floatingBubbleContent: 'limitsAllSessions',
    floatingBubbleScale: 1,
    themePreset: 'default',
    zoomFactor: 1,
    showCompactTotalTokens: false,
    windowsBackdrop: 'acrylic',
    language: 'auto',
    homeQuotaSelections: {},
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
  quotaLoading: false,
  statsGeneration: 0,
  lastRefreshAt: 0,
  viewMenuOpen: false,
  providerFilter: [],
  providerFilterMenuOpen: false,
  alwaysOnTop: true,
  openSession: null,
  detailSort: 'time',
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
      <div class="period-controls">
        <nav class="tabs" aria-label="Period tabs">
          <span class="tab-indicator" aria-hidden="true"></span>
          <button class="tab active" data-period="today" data-period-slot="today">DAY</button>
          <button id="monthPeriodTab" class="tab" data-period-slot="month" aria-haspopup="menu" aria-expanded="false">MONTH</button>
          <button class="tab" data-period="allTime" data-period-slot="allTime">TOTAL</button>
        </nav>
        <div id="monthPeriodMenu" class="view-switcher-menu period-menu hidden" role="menu" aria-labelledby="monthPeriodTab">
          <button class="view-switcher-menu-item" type="button" data-fixed-period="month" data-i18n="period.month">This month</button>
          <button class="view-switcher-menu-item" type="button" data-fixed-period="week" data-i18n="period.week">This week</button>
          <button class="view-switcher-menu-item" type="button" data-fixed-period="last7" data-i18n="period.last7">Last 7 days</button>
          <button class="view-switcher-menu-item" type="button" data-fixed-period="last30" data-i18n="period.last30">Last 30 days</button>
        </div>
      </div>
      <div class="window-control-region">
        <div class="window-actions">
          <button id="pinButton" class="icon-button" data-i18n-title="window.pin" title="Cycle window behavior">⇧</button>
          <button id="minButton" class="icon-button" data-i18n-title="window.minimize" title="Minimize">−</button>
          <button id="closeButton" class="icon-button" data-i18n-title="window.quit" title="Quit Token Lens">×</button>
        </div>
      </div>
    </header>
    <section id="settingsPanel" class="settings-panel hidden" aria-hidden="true">
      <div class="settings-group settings-collapsible-group v2-settings-card">
        <div class="settings-group-header"><span data-i18n="settings.floatingTray">Floating & Tray</span></div>
        <label class="checkbox-label settings-item">
          <span class="settings-item-text"><span class="settings-item-title" data-i18n="settings.trayIcon">Tray Icon</span></span>
          <input id="showTrayIconInput" type="checkbox" />
          <span class="settings-note settings-item-desc" data-i18n="settings.trayDesc">Keep Token Lens available from the system tray or menu bar.</span>
        </label>
        <label class="checkbox-label settings-item">
          <span class="settings-item-text"><span class="settings-item-title" data-i18n="settings.bubble">Floating Bubble</span></span>
          <input id="floatingBubbleInput" type="checkbox" />
          <span class="settings-note settings-item-desc" data-i18n="settings.bubbleDesc">Use the minimize button to collapse Token Lens into a draggable quota monitor.</span>
        </label>
        <div id="floatingBubbleOptions" class="presence-feature-body hidden">
          <div class="settings-item">
            <span id="floatingBubbleTriggerLabel" class="settings-item-text"><span class="settings-item-title" data-i18n="settings.openOn">Open on</span></span>
            <div class="inline-options" role="radiogroup" aria-labelledby="floatingBubbleTriggerLabel">
              <label class="inline-option"><input type="radio" name="floatingBubbleTrigger" value="click" /><span data-i18n="settings.click">Click</span></label>
              <label class="inline-option"><input type="radio" name="floatingBubbleTrigger" value="hover" /><span data-i18n="settings.hover">Hover</span></label>
            </div>
          </div>
          <label class="settings-item" for="floatingBubbleContentInput">
            <span class="settings-item-text"><span class="settings-item-title" data-i18n="settings.bubbleDisplay">Bubble display</span></span>
            <select id="floatingBubbleContentInput">
              <option value="limitsAllSessions" data-i18n="settings.bubble.providerLimits">Provider limits</option>
              <option value="icon" data-i18n="settings.bubble.icon">Icon only</option>
              <option value="barsSession" data-i18n="settings.bubble.lowestSession">Lowest session</option>
              <option value="barsWeekly" data-i18n="settings.bubble.lowestWeekly">Lowest weekly</option>
              <option value="barsAllSessions" data-i18n="settings.bubble.firstTwoBars">First two provider bars</option>
              <option value="bars" data-i18n="settings.bubble.lowestRemaining">Lowest remaining</option>
            </select>
          </label>
          <div class="settings-item settings-slider-item">
            <span class="settings-item-text"><span class="settings-item-title" data-i18n="settings.bubbleSize">Bubble size</span></span>
            <input id="floatingBubbleScaleInput" type="range" min="70" max="150" step="10" value="100" aria-label="Bubble size percentage" />
            <span id="floatingBubbleScaleValue" class="slider-value">100%</span>
          </div>
        </div>
      </div>
      <div class="settings-group settings-collapsible-group v2-settings-card">
        <div class="settings-group-header"><span data-i18n="settings.appearance">Appearance</span></div>
        <label class="settings-item" for="languageInput">
          <span class="settings-item-text"><span class="settings-item-title" data-i18n="settings.language">Interface language</span></span>
          <select id="languageInput">
            <option value="auto" data-i18n="settings.language.auto">Auto (system)</option>
            <option value="en" data-i18n="settings.language.en">English</option>
            <option value="ko" data-i18n="settings.language.ko">한국어</option>
            <option value="ja" data-i18n="settings.language.ja">日本語</option>
            <option value="zh-CN" data-i18n="settings.language.zh-CN">简体中文</option>
            <option value="zh-TW" data-i18n="settings.language.zh-TW">繁體中文</option>
          </select>
        </label>
        <div class="settings-item v2-theme-item">
          <span class="settings-item-text"><span class="settings-item-title" data-i18n="settings.theme">Theme</span></span>
          <div id="themePresetChips" class="theme-preset-chips" role="radiogroup" aria-label="Interface theme"></div>
        </div>
        <div class="settings-item settings-slider-item v2-zoom-item">
          <span class="settings-item-text"><span class="settings-item-title" data-i18n="settings.zoom">Zoom</span></span>
          <input id="zoomInput" type="range" min="70" max="160" step="10" value="100" aria-label="Zoom percentage" />
          <span id="zoomValue" class="slider-value">100%</span>
        </div>
        <label class="checkbox-label settings-item">
          <span class="settings-item-text"><span class="settings-item-title" data-i18n="settings.compactTotal">Compact Total</span></span>
          <input id="compactTotalInput" type="checkbox" />
          <span class="settings-note settings-item-desc" data-i18n="settings.compactTotalDesc">Show an approximate K/M/B total beside the full token count.</span>
        </label>
        <div id="windowsBackdropRow" class="settings-item hidden">
          <span id="windowsBackdropLabel" class="settings-item-text"><span class="settings-item-title" data-i18n="settings.backdrop">Windows Backdrop</span></span>
          <div class="inline-options" role="radiogroup" aria-labelledby="windowsBackdropLabel">
            <label class="inline-option"><input type="radio" name="windowsBackdrop" value="off" /><span data-i18n="settings.off">Off</span></label>
            <label class="inline-option"><input type="radio" name="windowsBackdrop" value="acrylic" /><span data-i18n="settings.acrylic">Acrylic</span></label>
          </div>
        </div>
      </div>
      <div class="settings-group settings-collapsible-group v2-settings-card">
        <div class="settings-group-header"><span data-i18n="settings.troubleshooting">Troubleshooting</span></div>
        <div class="settings-item">
          <span class="settings-item-text">
            <span class="settings-item-title" data-i18n="settings.errorLogs">Diagnostic logs</span>
            <span class="settings-note settings-item-desc" data-i18n="settings.errorLogsDesc">Startup timing keeps the latest 10 runs; provider error incidents are kept for up to 3 days.</span>
          </span>
          <div class="settings-actions"><button id="openErrorLogsButton" type="button" data-i18n="settings.openErrorLogs">Open diagnostic log folder</button></div>
        </div>
      </div>
    </section>
    <section class="total-panel">
      <div class="label-row"><span data-i18n="dashboard.totalTokens">TOTAL TOKENS</span></div>
      <div class="total-number-row"><div id="totalTokens" class="total-number">0</div><span id="totalTokensCompact" class="total-compact hidden"></span></div>
      <div id="cost" class="cost">$0.00</div>
    </section>
    <section id="homePanel" class="home-panel"></section>
    <section id="breakdown" class="breakdown hidden"></section>
    <div id="sessionDetailHead" class="detail-head hidden"></div>
    <div id="sessionDetail" class="session-detail hidden"></div>
    <section id="limitsPanel" class="limits-panel hidden"></section>
    <footer class="footer">
      <div id="viewSwitcher" class="view-switcher"></div>
      <span id="footerActionSlot">
        <span class="utility-actions">
          <button id="refreshButton" class="refresh-button" data-i18n-title="settings.refresh" data-i18n-aria-label="settings.refresh" title="Refresh" aria-label="Refresh"><span class="refresh-button-icon" aria-hidden="true">↻</span><span class="refresh-button-spinner" aria-hidden="true"></span></button>
          <button id="settingsButton" class="icon-button settings-icon-button" data-i18n-title="settings.settings" data-i18n-aria-label="settings.settings" title="Settings" aria-label="Settings"></button>
        </span>
      </span>
    </footer>
  </main>
  <button id="floatingBubbleTab" class="floating-bubble-tab" type="button" aria-hidden="true"><div id="floatingBubbleContent" class="floating-bubble-content is-icon"><span>Σ</span></div></button>
`;

const els = {
  shell: document.querySelector('#shell'),
  status: document.querySelector('#status'),
  liveDot: document.querySelector('#liveDot'),
  totalTokens: document.querySelector('#totalTokens'),
  totalTokensCompact: document.querySelector('#totalTokensCompact'),
  cost: document.querySelector('#cost'),
  monthPeriodTab: document.querySelector('#monthPeriodTab'),
  monthPeriodMenu: document.querySelector('#monthPeriodMenu'),
  homePanel: document.querySelector('#homePanel'),
  breakdown: document.querySelector('#breakdown'),
  sessionDetailHead: document.querySelector('#sessionDetailHead'),
  sessionDetail: document.querySelector('#sessionDetail'),
  limitsPanel: document.querySelector('#limitsPanel'),
  settingsPanel: document.querySelector('#settingsPanel'),
  settingsButton: document.querySelector('#settingsButton'),
  openErrorLogsButton: document.querySelector('#openErrorLogsButton'),
  languageInput: document.querySelector('#languageInput'),
  showTrayIconInput: document.querySelector('#showTrayIconInput'),
  floatingBubbleInput: document.querySelector('#floatingBubbleInput'),
  floatingBubbleOptions: document.querySelector('#floatingBubbleOptions'),
  floatingBubbleTriggerInputs: Array.from(document.querySelectorAll('input[name="floatingBubbleTrigger"]')),
  floatingBubbleContentInput: document.querySelector('#floatingBubbleContentInput'),
  floatingBubbleScaleInput: document.querySelector('#floatingBubbleScaleInput'),
  floatingBubbleScaleValue: document.querySelector('#floatingBubbleScaleValue'),
  floatingBubbleContent: document.querySelector('#floatingBubbleContent'),
  themePresetChips: document.querySelector('#themePresetChips'),
  zoomInput: document.querySelector('#zoomInput'),
  zoomValue: document.querySelector('#zoomValue'),
  compactTotalInput: document.querySelector('#compactTotalInput'),
  windowsBackdropRow: document.querySelector('#windowsBackdropRow'),
  windowsBackdropInputs: Array.from(document.querySelectorAll('input[name="windowsBackdrop"]')),
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

function recordStartupTiming(phase) {
  const elapsedMs = Math.round((performance.now() - APP_BOOTSTRAP_STARTED_AT) * 10) / 10;
  console.info('[Token Lens startup]', { phase, elapsedMs });
  void window.tokenMonitor.recordStartupTiming(phase).catch(() => {});
}

function renderThemePresetControls() {
  const active = state.settings.themePreset;
  els.themePresetChips.replaceChildren(...THEME_PRESETS.map((preset) => {
    const chip = document.createElement('button');
    chip.type = 'button';
    chip.className = `theme-preset-chip${preset.id === active ? ' active' : ''}`;
    chip.setAttribute('role', 'radio');
    chip.setAttribute('aria-checked', String(preset.id === active));
    const dot = document.createElement('span');
    dot.className = 'theme-preset-dot';
    dot.style.background = preset.accent;
    const label = document.createElement('span');
    label.textContent = preset.id === 'default' ? t('settings.theme.default') : preset.label;
    chip.append(dot, label);
    chip.addEventListener('click', () => void saveSettings({ themePreset: preset.id }));
    return chip;
  }));
}

function applySettings(settings = {}) {
  state.settings = {
    ...state.settings,
    ...settings,
    showTrayIcon: settings.showTrayIcon !== false,
    floatingBubbleEnabled: settings.floatingBubbleEnabled !== false,
    floatingBubbleTrigger: settings.floatingBubbleTrigger === 'hover' ? 'hover' : 'click',
    floatingBubbleContent: normalizeBubbleContent(settings.floatingBubbleContent ?? state.settings.floatingBubbleContent),
    floatingBubbleScale: normalizeBubbleScale(settings.floatingBubbleScale ?? state.settings.floatingBubbleScale),
    themePreset: normalizeThemePreset(settings.themePreset ?? state.settings.themePreset),
    zoomFactor: normalizeZoomFactor(settings.zoomFactor ?? state.settings.zoomFactor),
    showCompactTotalTokens: settings.showCompactTotalTokens === true,
    windowsBackdrop: settings.windowsBackdrop === 'off' ? 'off' : 'acrylic',
    language: normalizeLanguage(settings.language ?? state.settings.language),
    homeQuotaSelections: normalizeHomeQuotaSelections(settings.homeQuotaSelections ?? state.settings.homeQuotaSelections),
  };
  activeLanguage = resolveLanguage(state.settings.language, navigator.languages);
  activeLocale = state.settings.language === 'auto'
    ? (navigator.languages?.[0] || navigator.language || activeLanguage)
    : activeLanguage;
  document.documentElement.lang = activeLanguage;
  document.documentElement.style.setProperty('--bubble-scale', String(state.settings.floatingBubbleScale));
  applyTranslations(document, activeLanguage);
  applyThemePreset(document.documentElement, state.settings.themePreset);
  els.showTrayIconInput.checked = state.settings.showTrayIcon;
  els.languageInput.value = state.settings.language;
  els.floatingBubbleInput.checked = state.settings.floatingBubbleEnabled;
  els.floatingBubbleOptions.classList.toggle('hidden', !state.settings.floatingBubbleEnabled);
  els.floatingBubbleContentInput.value = state.settings.floatingBubbleContent;
  els.floatingBubbleScaleInput.value = String(Math.round(state.settings.floatingBubbleScale * 100));
  els.floatingBubbleScaleValue.textContent = `${els.floatingBubbleScaleInput.value}%`;
  els.minButton.title = state.settings.floatingBubbleEnabled
    ? t('window.minimizeBubble')
    : state.settings.showTrayIcon ? t('window.minimizeTray') : t('window.minimize');
  els.closeButton.title = t('window.quit');
  els.zoomInput.value = String(Math.round(state.settings.zoomFactor * 100));
  els.zoomValue.textContent = `${els.zoomInput.value}%`;
  els.compactTotalInput.checked = state.settings.showCompactTotalTokens;
  els.windowsBackdropRow.classList.toggle('hidden', !isWindows);
  for (const input of els.floatingBubbleTriggerInputs) input.checked = input.value === state.settings.floatingBubbleTrigger;
  for (const input of els.windowsBackdropInputs) input.checked = input.value === state.settings.windowsBackdrop;
  renderThemePresetControls();
  if (state.stats) renderHeadline();
}

function bubbleProviderIcon(selection) {
  const icon = document.createElement('span');
  icon.className = `bubble-provider-icon row-icon ${selection.iconClass || ''}`;
  if (selection.color) icon.style.color = selection.color;
  icon.setAttribute('aria-hidden', 'true');
  return icon;
}

function bubbleBar(percent) {
  const track = document.createElement('span');
  track.className = 'bubble-bar-track';
  const fill = document.createElement('span');
  fill.className = 'bubble-bar-fill';
  const value = Number(percent);
  fill.style.setProperty('--bubble-fill', String(Number.isFinite(value) ? Math.max(0, Math.min(1, value / 100)) : 0));
  track.append(fill);
  return track;
}

function renderFloatingBubbleContent() {
  const content = els.floatingBubbleContent;
  const model = floatingBubbleModel(state.stats?.limits, state.settings.floatingBubbleContent);
  content.className = `floating-bubble-content is-${model.kind}`;
  if (model.kind === 'icon') {
    const mark = document.createElement('span');
    mark.className = 'bubble-sigma';
    mark.textContent = 'Σ';
    content.replaceChildren(mark);
    return;
  }
  if (model.kind === 'limits') {
    const nodes = [];
    model.entries.forEach((entry, index) => {
      if (index) {
        const separator = document.createElement('span');
        separator.className = 'bubble-separator';
        separator.textContent = '·';
        nodes.push(separator);
      }
      const item = document.createElement('span');
      item.className = 'bubble-limit-entry';
      const text = document.createElement('span');
      text.className = 'bubble-limit-text';
      text.textContent = entry.percents.map(bubblePercentLabel).filter(Boolean).join(' · ');
      item.append(bubbleProviderIcon(entry), text);
      nodes.push(item);
    });
    content.replaceChildren(...nodes);
    return;
  }
  const stack = document.createElement('span');
  stack.className = 'bubble-bars-stack';
  if (model.kind === 'providerBars') {
    stack.append(bubbleBar(model.primaryPercent), bubbleBar(model.secondaryPercent));
    content.replaceChildren(bubbleProviderIcon(model), stack);
    return;
  }
  for (const percent of model.percents || []) stack.append(bubbleBar(percent));
  content.replaceChildren(stack);
}

function measureFloatingBubbleWidth() {
  const probe = els.floatingBubbleContent.cloneNode(true);
  probe.classList.add('floating-bubble-measure');
  document.body.append(probe);
  const width = Math.ceil(probe.getBoundingClientRect().width);
  probe.remove();
  const scale = state.settings.floatingBubbleScale;
  const minWidth = BUBBLE_LOGICAL_HEIGHT * scale;
  const maxWidth = BUBBLE_MAX_WIDTH * scale;
  return Math.max(minWidth, Math.min(maxWidth, width || minWidth));
}

async function syncFloatingBubbleWidth() {
  renderFloatingBubbleContent();
  return window.tokenMonitor.setFloatingBubbleWidth(measureFloatingBubbleWidth());
}

function applyFloatingBubbleState(payload = {}, { renderContent = true } = {}) {
  const side = payload.collapsed && ['left', 'right'].includes(payload.side) ? payload.side : null;
  state.floatingBubble = { collapsed: Boolean(side), side };
  for (const node of [document.documentElement, document.body]) {
    node.classList.toggle('floating-bubble-collapsed-left', side === 'left');
    node.classList.toggle('floating-bubble-collapsed-right', side === 'right');
  }
  els.floatingBubbleTab.setAttribute('aria-hidden', String(!side));
  els.floatingBubbleTab.title = side ? t('window.expand') : '';
  if (side) {
    state.settingsOpen = false;
    els.shell.classList.remove('settings-open');
    els.settingsPanel.classList.add('hidden');
    els.settingsPanel.setAttribute('aria-hidden', 'true');
  }
  if (renderContent) renderFloatingBubbleContent();
}

async function saveSettings(patch) {
  const settings = await window.tokenMonitor.updateSettings(patch);
  applySettings(settings);
  const bubble = await window.tokenMonitor.getFloatingBubbleState();
  applyFloatingBubbleState(bubble);
  await syncFloatingBubbleWidth();
  renderSurface();
  return settings;
}

function currentPeriod() {
  return state.stats?.periods?.[state.period] || { totalTokens: 0, costUsd: 0 };
}

function localizedQuotaWindowLabel(window) {
  const label = quotaWindowLabel(window);
  if (window?.metric === 'spend' && /^Usage credits$/i.test(label)) return t('quota.usageCredits');
  if (label === '5-hour') return t('quota.fiveHour');
  if (label === 'Weekly') return t('quota.weekly');
  if (label === 'Monthly') return t('quota.monthly');
  if (label === 'Quota') return t('quota.quota');
  if (label === 'Additional') return t('quota.additional');
  return label;
}

function localizedResetTime(value) {
  const reset = formatResetTime(value, new Date(), currentLocale());
  return reset ? `${t('quota.resets')} ${reset.replace(/^Resets\s+/, '')}` : '';
}

function quotaMoney(value, currency = 'USD') {
  const number = Number(value);
  if (!Number.isFinite(number)) return '';
  const code = String(currency || 'USD').trim().toUpperCase() || 'USD';
  try {
    return new Intl.NumberFormat(currentLocale(), {
      style: 'currency',
      currency: code,
      minimumFractionDigits: Number.isInteger(number) ? 0 : 1,
      maximumFractionDigits: 1,
    }).format(number);
  } catch (_) {
    return `${code} ${number.toLocaleString(currentLocale(), { maximumFractionDigits: 1 })}`;
  }
}

function quotaWindowValue(window, { detail = false } = {}) {
  const percent = window?.remainingPercent == null ? '' : formatPercent(window.remainingPercent);
  if (window?.metric === 'spend' && window?.used != null) {
    const remaining = window.remaining == null ? '' : quotaMoney(window.remaining, window.currency);
    const limit = window.limit == null ? '' : quotaMoney(window.limit, window.currency);
    const absolute = remaining && limit ? `${remaining}/${limit}` : quotaMoney(window.used, window.currency);
    return percent ? `${detail ? t('quota.left', { value: percent }) : percent} · ${absolute}` : absolute;
  }
  if (window?.metric === 'credits' && window?.currency === 'CREDITS') {
    const count = formatQuotaCount(window);
    const credits = count ? t('quota.credits', { value: count }) : '';
    if (credits) return percent ? `${detail ? t('quota.left', { value: percent }) : percent} · ${credits}` : credits;
  }
  if (!percent) return '—';
  return detail ? t('quota.left', { value: percent }) : percent;
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
  for (const button of periodMenuButtons()) {
    const active = button.dataset.fixedPeriod === activeMode;
    button.classList.toggle('is-current', active);
    button.setAttribute('aria-checked', String(active));
    if (active) button.setAttribute('aria-current', 'true');
    else button.removeAttribute('aria-current');
    button.tabIndex = state.periodMenuOpen && active ? 0 : -1;
  }
}

function syncPeriodIndicator() {
  const tabs = document.querySelector('.period-controls .tabs');
  const activeTab = tabs?.querySelector('.tab.active');
  if (!tabs || !activeTab) return;
  const inset = 1;
  const left = Math.max(0, activeTab.offsetLeft + inset);
  const width = Math.max(0, activeTab.offsetWidth - inset * 2);
  tabs.style.setProperty('--period-indicator-left', `${left}px`);
  tabs.style.setProperty('--period-indicator-width', `${width}px`);
}

function syncPeriodTabs() {
  const activeSlot = slotForSelection(state.period);
  const tabs = Array.from(document.querySelectorAll('.tab'));
  for (const tab of tabs) {
    const active = tab.dataset.periodSlot === activeSlot;
    tab.classList.toggle('active', active);
    tab.setAttribute('aria-pressed', String(active));
  }
  const mode = activeSlot === 'month' ? normalizeMonthMode(state.period) : normalizeMonthMode(state.monthMode);
  els.monthPeriodTab.textContent = periodDisplayLabel(mode);
  syncPeriodIndicator();
  syncPeriodMenu();
}

function renderHeadline() {
  const period = currentPeriod();
  const filterApplies = ['model', 'session'].includes(state.view) && state.providerFilter.length > 0;
  const totals = filterApplies
    ? providerFilteredTotals(period, state.providerFilter)
    : { totalTokens: period.totalTokens, costUsd: period.costUsd };
  const total = Math.max(0, Number(totals.totalTokens) || 0);
  els.totalTokens.textContent = formatNumber(total);
  const compactLabel = compactTotalLabel(total, state.settings.showCompactTotalTokens);
  els.totalTokensCompact.textContent = compactLabel;
  els.totalTokensCompact.classList.toggle('hidden', !compactLabel);
  els.cost.textContent = formatCost(totals.costUsd);
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
  const { module, body } = homeModule(t('home.models'), 'model', 'view-icon-model');
  const rows = modelRows(currentPeriod()).slice(0, 5);
  if (!rows.length) {
    const empty = document.createElement('div');
    empty.className = 'home-module-empty';
    empty.textContent = t('home.noModelUsage');
    body.append(empty);
    return module;
  }
  for (const row of rows) body.append(homeListRow(row, 'model'));
  return module;
}

function renderHomeLimits() {
  const { module, body } = homeModule(t('home.limits'), 'limits', 'view-icon-limits');
  const rows = homeQuotaRows(state.stats?.limits, state.settings.homeQuotaSelections);
  if (!rows.length) {
    const empty = document.createElement('div');
    empty.className = 'home-module-empty';
    empty.textContent = state.quotaLoading ? t('common.loading') : t('home.noLimits');
    body.append(empty);
    return module;
  }
  for (const row of rows) {
    const account = document.createElement('div');
    account.className = 'home-limit-account';
    const head = document.createElement('div');
    head.className = 'home-limit-account-head';
    const mark = iconSpan(row.iconClass, row.color);
    mark.classList.add('home-list-mark');
    const name = document.createElement('span');
    name.className = 'home-list-name';
    name.textContent = [row.name, row.plan, row.status === 'stale' ? t('common.stale') : '']
      .filter(Boolean).join(' · ');
    head.append(mark, name);
    const windows = document.createElement('div');
    windows.className = 'home-limit-windows';
    const compactWindows = homeQuotaWindows(row, state.settings.homeQuotaSelections);
    if (!compactWindows.length) {
      const empty = document.createElement('div');
      empty.className = 'home-module-empty';
      empty.textContent = row.status === 'ok' ? t('common.noQuotaWindows') : t('common.unavailable');
      windows.append(empty);
    }
    for (const window of compactWindows) {
      const item = document.createElement('div');
      item.className = 'home-limit-window';
      if (window.metric === 'credits' || window.metric === 'spend') item.classList.add('home-limit-window-wide');
      const line = document.createElement('div');
      line.className = 'home-limit-window-line';
      const label = document.createElement('span');
      label.className = 'home-limit-window-label';
      label.textContent = localizedQuotaWindowLabel(window);
      const value = document.createElement('span');
      value.className = 'home-list-value';
      value.textContent = quotaWindowValue(window);
      if (window.remainingPercent != null && window.remainingPercent < 20) value.classList.add('home-limit-value-critical');
      else if (window.remainingPercent != null && window.remainingPercent < 50) {
        value.classList.add('home-limit-value-low');
      }
      line.append(label, value);
      item.append(line);
      const reset = localizedResetTime(window.resetsAt);
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

function shortHistoryDate(key) {
  const value = new Date(`${String(key).slice(0, 10)}T00:00:00Z`);
  if (Number.isNaN(value.getTime())) return String(key || '');
  return new Intl.DateTimeFormat(currentLocale(), { month: 'short', day: 'numeric', timeZone: 'UTC' }).format(value);
}

function historyHeatmapSvg(heatmap) {
  const width = Math.max(300, Number(heatmap?.width || 0));
  const height = Math.max(1, Number(heatmap?.height || 0));
  const months = (heatmap?.monthLabels || []).map((month) => {
    const x = month.col * ((heatmap?.cell || 9) + (heatmap?.gap || 3));
    const label = new Intl.DateTimeFormat(currentLocale(), { month: 'short', timeZone: 'UTC' })
      .format(new Date(`${month.date}T00:00:00Z`));
    return `<text class="heat-month" x="${x}" y="9">${label}</text>`;
  }).join('');
  const cells = (heatmap?.cells || []).map((cell) => {
    const level = Math.max(0, Math.min(4, Number(cell.intensity) || 0));
    const title = `${shortHistoryDate(cell.date)} · ${t('history.tokens', { value: formatCompact(cell.tokens) })}`;
    return `<rect class="heat lvl-${level}" x="${cell.x}" y="${cell.y}" width="${cell.size}" height="${cell.size}" rx="2"><title>${title}</title></rect>`;
  }).join('');
  return `<svg class="dash-heatmap" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" aria-label="Token usage activity by day">${months}${cells}</svg>`;
}

function historyTrendSvg(trend) {
  if (!trend?.line) return '';
  return `<svg class="area-line" viewBox="0 0 ${trend.width} ${trend.height}" preserveAspectRatio="none" aria-label="Recent token usage trend"><defs><linearGradient id="area-line-grad" x1="0" y1="0" x2="0" y2="1"><stop class="area-line-grad-top" offset="0%"></stop><stop class="area-line-grad-bottom" offset="100%"></stop></linearGradient></defs><path class="area-line-fill" d="${trend.area}"></path><path class="area-line-stroke" d="${trend.line}"></path></svg>`;
}

function renderHomeActivity() {
  const module = document.createElement('section');
  module.className = 'home-module home-module-trends v2-history-module';
  const head = document.createElement('div');
  head.className = 'home-module-head';
  const label = document.createElement('span');
  label.className = 'home-module-label';
  label.textContent = t('home.activity');
  const meta = document.createElement('span');
  meta.className = 'home-module-meta';
  head.append(label, meta);
  const body = document.createElement('div');
  body.className = 'home-module-body';
  module.append(head, body);

  if (state.historyLoading && !state.history) {
    meta.textContent = t('home.loading');
    const empty = document.createElement('div');
    empty.className = 'home-module-empty';
    empty.textContent = t('home.loadingHistory');
    body.append(empty);
    return module;
  }
  if (state.historyError && !state.history) {
    meta.textContent = t('common.unavailable');
    const empty = document.createElement('div');
    empty.className = 'home-module-empty';
    empty.textContent = t('home.historyUnavailable');
    body.append(empty);
    return module;
  }
  if (!state.history) {
    meta.textContent = '';
    return module;
  }

  const view = historyViewModel(state.history, state.stats?.periods?.today || {});
  meta.textContent = t('home.activeDays', { count: formatNumber(view.activeDays) });
  if (!view.daily.some((day) => Number(day.tokens) > 0)) {
    const empty = document.createElement('div');
    empty.className = 'home-module-empty';
    empty.textContent = t('home.noHistory');
    body.append(empty);
    return module;
  }
  const activityScroll = document.createElement('div');
  activityScroll.className = 'home-activity-scroll';
  activityScroll.tabIndex = 0;
  const canvas = document.createElement('div');
  canvas.className = 'home-activity-canvas';
  canvas.innerHTML = historyHeatmapSvg(view.heatmap);
  activityScroll.append(canvas);

  const trendHead = document.createElement('div');
  trendHead.className = 'home-trend-head';
  const trendTitle = document.createElement('span');
  trendTitle.textContent = t('home.trend');
  const peak = document.createElement('span');
  peak.className = 'home-module-meta';
  peak.textContent = t('home.peak', { value: formatCompact(view.peakDayTokens) });
  trendHead.append(trendTitle, peak);
  const plot = document.createElement('div');
  plot.className = 'home-trend-plot';
  const chart = document.createElement('div');
  chart.className = 'home-area-chart';
  chart.innerHTML = historyTrendSvg(view.trend);
  plot.append(chart);
  const dates = document.createElement('div');
  dates.className = 'home-trend-dates';
  for (const date of view.trend.dates) {
    const item = document.createElement('span');
    item.className = 'home-trend-date';
    item.textContent = shortHistoryDate(date);
    dates.append(item);
  }
  body.append(activityScroll, trendHead, plot, dates);
  activityScroll.addEventListener('scroll', () => {
    const max = Math.max(0, activityScroll.scrollWidth - activityScroll.clientWidth);
    if (max <= 0) return;
    state.historyScrollLeft = activityScroll.scrollLeft;
    state.historyFollowEnd = activityScroll.scrollLeft >= max - 2;
  }, { passive: true });
  queueMicrotask(() => {
    const max = Math.max(0, activityScroll.scrollWidth - activityScroll.clientWidth);
    activityScroll.scrollLeft = state.historyFollowEnd || state.historyScrollLeft == null
      ? max
      : Math.max(0, Math.min(max, state.historyScrollLeft));
  });
  return module;
}

function renderHome() {
  els.homePanel.replaceChildren(renderHomeLimits(), renderHomeModels(), renderHomeActivity());
  void loadDashboardHistory();
}
function breakdownRow(row, max, kind) {
  const item = document.createElement('div');
  item.className = `row${kind === 'session' ? ' session-row' : ''}`;
  item.dataset.key = row.key;
  if (kind === 'session') item.dataset.client = row.client || '';
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
    if (kind === 'session' && row.messageCount > 0) {
      detail.textContent = [row.context, t('session.messages', { count: formatNumber(row.messageCount) })]
        .filter(Boolean).join(' · ');
    } else {
      detail.textContent = row.detail;
    }
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
  if (kind === 'session' && ['codex', 'claude'].includes(row.client) && row.sessionId) {
    item.tabIndex = 0;
    item.setAttribute('role', 'button');
    item.setAttribute('aria-label', `Open usage detail for ${row.name}`);
    item.addEventListener('click', () => void openSessionDetail(row));
    item.addEventListener('keydown', (event) => {
      if (!['Enter', ' '].includes(event.key)) return;
      event.preventDefault();
      void openSessionDetail(row);
    });
  }
  return item;
}

function rowsForView() {
  const period = currentPeriod();
  if (state.view === 'tool') return toolRows(period);
  if (state.view === 'model') return modelRows(period, state.providerFilter);
  if (state.view === 'session') return sessionRows(period, state.providerFilter);
  return [];
}

function providerFilterLabel() {
  if (!state.providerFilter.length) return t('filter.all');
  const labels = state.providerFilter.map((provider) => clientLabel(provider).replace(' Code', ''));
  if (labels.length <= 2) return labels.join(' + ');
  return t('filter.providersCount', { count: labels.length });
}

function setProviderFilter(next) {
  state.providerFilter = PROVIDER_FILTER_ORDER.filter((provider) => next.includes(provider));
  state.openSession = null;
  render();
}

function renderProviderFilter() {
  const wrap = document.createElement('div');
  wrap.className = `provider-filter${state.providerFilterMenuOpen ? ' is-open' : ''}`;
  const button = document.createElement('button');
  button.type = 'button';
  button.className = 'provider-filter-button';
  button.setAttribute('aria-haspopup', 'menu');
  button.setAttribute('aria-expanded', String(state.providerFilterMenuOpen));
  button.setAttribute('aria-label', t('filter.provider'));
  const buttonLabel = document.createElement('span');
  buttonLabel.textContent = providerFilterLabel();
  const arrow = document.createElement('span');
  arrow.className = 'provider-filter-arrow';
  arrow.textContent = '▾';
  button.append(buttonLabel, arrow);
  button.addEventListener('click', () => {
    state.providerFilterMenuOpen = !state.providerFilterMenuOpen;
    renderBreakdown();
  });
  wrap.append(button);
  if (state.providerFilterMenuOpen) {
    const menu = document.createElement('div');
    menu.className = 'provider-filter-menu';
    menu.setAttribute('role', 'menu');
    const all = document.createElement('button');
    all.type = 'button';
    all.className = `provider-filter-item${state.providerFilter.length ? '' : ' is-selected'}`;
    all.setAttribute('role', 'menuitemcheckbox');
    all.setAttribute('aria-checked', String(state.providerFilter.length === 0));
    all.textContent = `${state.providerFilter.length ? '○' : '✓'} ${t('filter.all')}`;
    all.addEventListener('click', () => setProviderFilter([]));
    menu.append(all);
    for (const provider of PROVIDER_FILTER_ORDER) {
      const selected = state.providerFilter.includes(provider);
      const item = document.createElement('button');
      item.type = 'button';
      item.className = `provider-filter-item${selected ? ' is-selected' : ''}`;
      item.setAttribute('role', 'menuitemcheckbox');
      item.setAttribute('aria-checked', String(selected));
      item.textContent = `${selected ? '✓' : '○'} ${clientLabel(provider).replace(' Code', '')}`;
      item.addEventListener('click', () => {
        const next = selected
          ? state.providerFilter.filter((value) => value !== provider)
          : [...state.providerFilter, provider];
        setProviderFilter(next);
        state.providerFilterMenuOpen = true;
        renderBreakdown();
      });
      menu.append(item);
    }
    wrap.append(menu);
  }
  return wrap;
}
async function openSessionDetail(row) {
  if (!row?.sessionId || !['codex', 'claude'].includes(row.client)) return;
  const request = {
    client: row.client,
    sessionId: row.sessionId,
    sessionCost: Number(row.cost) || 0,
    title: row.name || row.sessionId,
    startTimeMs: periodStartTimeMs(state.period, { locale: currentLocale() }),
    loading: true,
    error: false,
    detail: null,
  };
  state.openSession = request;
  renderSurface();
  try {
    const detail = await window.tokenMonitor.getSessionDetail({
      client: request.client,
      sessionId: request.sessionId,
      startTimeMs: request.startTimeMs,
      sessionCost: request.sessionCost,
    });
    if (state.openSession !== request) return;
    request.loading = false;
    request.detail = detail;
    renderSurface();
  } catch (error) {
    console.error(error);
    if (state.openSession !== request) return;
    request.loading = false;
    request.error = true;
    renderSurface();
  }
}

function closeSessionDetail() {
  state.openSession = null;
  renderSurface();
}

function toggleSessionDetailSort() {
  state.detailSort = state.detailSort === 'tokens' ? 'time' : 'tokens';
  renderSessionDetail();
}

function detailNote(text) {
  const note = document.createElement('div');
  note.className = 'detail-note';
  note.textContent = text;
  return note;
}

function sessionTurnNode(turn) {
  const element = document.createElement('div');
  element.className = 'detail-turn';
  const label = document.createElement('div');
  label.className = 'detail-turn-label';
  const title = document.createElement('span');
  title.className = 'detail-turn-title';
  title.textContent = t('session.turn', { label: turn.label });
  const tokens = turn.tokens || {};
  const cache = (Number(tokens.cacheRead) || 0) + (Number(tokens.cacheWrite) || 0);
  const split = document.createElement('span');
  split.className = 'detail-turn-split';
  split.textContent = t('session.split', {
    input: formatNumber(tokens.input || 0),
    output: formatNumber(tokens.output || 0),
    cache: formatNumber(cache),
    reason: tokens.reasoning ? t('session.reason', { value: formatNumber(tokens.reasoning) }) : '',
  });
  const tools = document.createElement('span');
  tools.className = 'detail-turn-tools';
  tools.textContent = turn.tools ? `⊢ ${turn.tools}` : '';
  label.append(title, split, tools);

  const metrics = document.createElement('div');
  metrics.className = 'detail-turn-metrics';
  const value = document.createElement('span');
  value.className = 'detail-turn-value';
  value.textContent = formatNumber(turn.value);
  const cost = document.createElement('span');
  cost.className = 'detail-turn-cost';
  cost.textContent = formatCost(turn.cost);
  metrics.append(value, cost);
  element.append(label, metrics);
  return element;
}

function sessionExchangeNode(row, max, color) {
  const wrap = document.createElement('div');
  wrap.className = 'detail-exchange';
  const head = document.createElement('div');
  head.className = 'detail-ex-head';
  const chevron = document.createElement('span');
  chevron.className = 'detail-chev';
  chevron.textContent = '▸';
  const label = document.createElement('div');
  label.className = 'detail-ex-label';
  const title = document.createElement('span');
  title.className = 'detail-ex-title';
  title.textContent = row.title;
  const subtitle = document.createElement('span');
  subtitle.className = 'detail-ex-sub';
  subtitle.textContent = row.subtitle;
  label.append(title, subtitle);
  const metrics = document.createElement('div');
  metrics.className = 'detail-ex-metrics';
  const value = document.createElement('span');
  value.className = 'detail-ex-value';
  value.textContent = formatNumber(row.value);
  const cost = document.createElement('span');
  cost.className = 'detail-ex-cost';
  cost.textContent = formatCost(row.cost);
  metrics.append(value, cost);
  head.append(chevron, label, metrics);

  const bar = document.createElement('div');
  bar.className = 'bar';
  const fill = document.createElement('div');
  fill.className = 'bar-fill';
  fill.style.background = color;
  fill.style.setProperty('--bar-scale', String(max > 0 ? Math.max(0, Math.min(1, row.value / max)) : 0));
  bar.append(fill);

  const turns = document.createElement('div');
  turns.className = 'detail-turns hidden';
  for (const turn of row.turns) turns.append(sessionTurnNode(turn));
  head.addEventListener('click', () => {
    const collapsed = turns.classList.toggle('hidden');
    chevron.textContent = collapsed ? '▸' : '▾';
  });
  wrap.append(head, bar, turns);
  return wrap;
}

function renderSessionDetail() {
  const request = state.openSession;
  if (!request) return;
  els.sessionDetailHead.replaceChildren();
  els.sessionDetail.replaceChildren();
  const back = document.createElement('button');
  back.type = 'button';
  back.className = 'detail-back';
  back.textContent = t('session.back');
  back.addEventListener('click', closeSessionDetail);
  els.sessionDetailHead.append(back);

  if (request.loading) {
    els.sessionDetail.append(detailNote(t('common.loading')));
    return;
  }
  if (request.error || request.detail?.found === false) {
    els.sessionDetail.append(detailNote(t('session.notFound')));
    return;
  }
  const rows = exchangeRows(request.detail, { now: new Date(), sortBy: state.detailSort });
  if (!rows.length) {
    els.sessionDetail.append(detailNote(t('session.noActivity')));
    return;
  }
  const sort = document.createElement('button');
  sort.type = 'button';
  sort.className = 'detail-sort';
  sort.textContent = state.detailSort === 'tokens' ? t('session.mostTokens') : t('session.newest');
  sort.addEventListener('click', toggleSessionDetailSort);
  els.sessionDetailHead.append(sort);
  const max = Math.max(1, ...rows.map((row) => row.value));
  const color = clientColor(request.client);
  els.sessionDetail.replaceChildren(...rows.map((row) => sessionExchangeNode(row, max, color)));
}

function renderBreakdown() {
  const rows = rowsForView();
  const max = Math.max(1, ...rows.map((row) => row.value));
  const nodes = [];
  if (['model', 'session'].includes(state.view)) nodes.push(renderProviderFilter());
  if (!rows.length) {
    const empty = document.createElement('div');
    empty.className = 'home-module-empty';
    empty.textContent = state.view === 'session' ? t('session.noSessionUsage') : t('common.noUsage');
    nodes.push(empty);
    els.breakdown.replaceChildren(...nodes);
    return;
  }
  nodes.push(...rows.map((row) => breakdownRow(row, max, state.view)));
  els.breakdown.replaceChildren(...nodes);
}

function hasExplicitHomeQuotaSelection(providerId) {
  return Object.prototype.hasOwnProperty.call(state.settings.homeQuotaSelections || {}, providerId);
}

function effectiveHomeQuotaSelectionKeys(row) {
  const providerId = String(row?.providerId || '').trim().toLowerCase();
  if (hasExplicitHomeQuotaSelection(providerId)) {
    return Array.isArray(state.settings.homeQuotaSelections[providerId])
      ? [...state.settings.homeQuotaSelections[providerId]]
      : [];
  }
  return homeQuotaWindows(row, {}).map((window) => quotaWindowSelectionKey(row, window));
}

async function setHomeQuotaWindowSelection(row, window, enabled) {
  const providerId = String(row?.providerId || '').trim().toLowerCase();
  if (!providerId) return;
  const key = quotaWindowSelectionKey(row, window);
  let keys = effectiveHomeQuotaSelectionKeys(row).filter((value) => value !== key);
  if (enabled) {
    if (keys.length >= 2) return;
    keys.push(key);
  }
  const selections = normalizeHomeQuotaSelections(state.settings.homeQuotaSelections);
  selections[providerId] = keys;
  await saveSettings({ homeQuotaSelections: selections });
}

function limitWindowNode(window, row) {
  const item = document.createElement('div');
  item.className = 'limit-window';
  const creditsDetail = window.metric === 'credits' && window.currency === 'CREDITS'
    ? formatQuotaCount(window)
    : '';
  if (window.additional || creditsDetail) item.classList.add('limit-window-wide');
  const text = document.createElement('div');
  text.className = `limit-window-text${row.providerId === 'gemini' ? ' limit-window-text-selectable' : ''}`;
  const label = document.createElement('span');
  label.textContent = localizedQuotaWindowLabel(window);
  const value = document.createElement('span');
  value.textContent = quotaWindowValue(window, { detail: true });
  if (row.providerId === 'gemini') {
    const selectionKeys = effectiveHomeQuotaSelectionKeys(row);
    const selectionKey = quotaWindowSelectionKey(row, window);
    const selected = selectionKeys.includes(selectionKey);
    const actions = document.createElement('div');
    actions.className = 'limit-window-actions';
    const toggle = document.createElement('label');
    toggle.className = 'limit-home-toggle';
    toggle.title = t('limits.showOnHome');
    const input = document.createElement('input');
    input.type = 'checkbox';
    input.checked = selected;
    input.disabled = !selected && selectionKeys.length >= 2;
    const caption = document.createElement('span');
    caption.textContent = t('limits.home');
    toggle.append(input, caption);
    toggle.classList.toggle('is-disabled', input.disabled);
    input.addEventListener('change', () => {
      input.disabled = true;
      void setHomeQuotaWindowSelection(row, window, input.checked);
    });
    actions.append(value, toggle);
    text.append(label, actions);
  } else {
    text.append(label, value);
  }
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
  const resetText = localizedResetTime(window.resetsAt);
  if (resetText || creditsDetail) {
    const reset = document.createElement('div');
    reset.className = 'limit-reset';
    if (creditsDetail) {
      reset.classList.add('limit-reset-split');
      const resetLabel = document.createElement('span');
      resetLabel.textContent = resetText;
      const detail = document.createElement('span');
      detail.className = 'limit-detail';
      detail.textContent = t('quota.credits', { value: creditsDetail });
      reset.append(resetLabel, detail);
    } else {
      reset.textContent = resetText;
    }
    item.append(reset);
  }
  return item;
}

function renderLimits() {
  const rows = quotaRows(state.stats?.limits);
  if (!rows.length && state.quotaLoading) {
    const loading = document.createElement('div');
    loading.className = 'home-module-empty';
    loading.textContent = t('common.loading');
    els.limitsPanel.replaceChildren(loading);
    return;
  }
  const nodes = rows.map((row) => {
    const item = document.createElement('div');
    item.className = 'limit-row';
    const head = document.createElement('div');
    head.className = 'limit-head';
    const name = document.createElement('div');
    name.className = 'limit-name';
    name.append(iconSpan(row.iconClass, row.color));
    const title = document.createElement('span');
    title.textContent = row.accountEmail ? `${row.name} · ${row.accountEmail}` : row.name;
    name.append(title);
    const plan = document.createElement('span');
    plan.className = 'limit-plan';
    plan.textContent = [
      row.plan,
      row.status === 'stale' ? t('common.stale') : row.status === 'unavailable' ? t('common.unavailable') : '',
      row.providerId === 'gemini' && !hasExplicitHomeQuotaSelection('gemini') ? t('limits.homeAuto') : '',
    ].filter(Boolean).join(' · ');
    head.append(name, plan);
    const windows = document.createElement('div');
    windows.className = `limit-windows${row.providerId === 'gemini' ? ' limit-windows-gemini' : ''}`;
    for (const window of row.windows) windows.append(limitWindowNode(window, row));
    if (!row.windows.length) {
      const empty = document.createElement('div');
      empty.className = 'home-module-empty';
      const stateLabel = row.status === 'ok' ? t('common.noQuotaWindows') : t('common.unavailable');
      empty.textContent = row.diagnostic ? `${stateLabel} · ${row.diagnostic}` : stateLabel;
      windows.append(empty);
    }
    if (row.providerId === 'codex' && row.resetCredits?.availableCount > 0) {
      const credits = document.createElement('div');
      credits.className = 'limit-window limit-window-note limit-window-wide';
      credits.innerHTML = `<div class="limit-window-text"><span>${t('quota.rateReset')}</span><span>${t('quota.available', { count: formatNumber(row.resetCredits.availableCount) })}</span></div>`;
      windows.append(credits);
    }
    item.append(head, windows);
    return item;
  });
  els.limitsPanel.replaceChildren(...nodes);
}
function setView(view) {
  if (!VIEW_ORDER.includes(view)) return;
  const changed = state.view !== view;
  state.view = view;
  if (view !== 'session') state.openSession = null;
  state.viewMenuOpen = false;
  state.providerFilterMenuOpen = false;
  render();
  if (changed && view === 'session') void refresh();
}

function statsRequestOptions(force = false, period = state.period) {
  const derived = derivedRequest(period, { locale: currentLocale() });
  return {
    force,
    includeSessionMetadata: state.view === 'session',
    ...(derived ? { derived } : {}),
  };
}

function setPeriod(period) {
  if (!PERIODS.includes(period)) return false;
  const changed = state.period !== period;
  state.period = period;
  if (changed) state.openSession = null;
  if (MONTH_PERIODS.includes(period)) state.monthMode = period;
  setPeriodMenuOpen(false);
  state.providerFilterMenuOpen = false;
  render();
  if (derivedRequest(period, { locale: currentLocale() })) void refresh();
  else if (['month', 'allTime'].includes(period) && !state.stats?.periods?.[period]) void loadPeriodIfMissing(period);
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
  label.textContent = t(meta.labelKey);
  current.append(icon, label);
  current.addEventListener('click', () => {
    const index = VIEW_ORDER.indexOf(state.view);
    setView(VIEW_ORDER[(index + 1) % VIEW_ORDER.length]);
  });
  const disclosure = document.createElement('button');
  disclosure.type = 'button';
  disclosure.className = 'view-switcher-disclosure';
  disclosure.setAttribute('aria-label', t('view.choose'));
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
    itemLabel.textContent = t(VIEW_META[view].labelKey);
    item.append(itemIcon, itemLabel);
    item.addEventListener('click', () => setView(view));
    menu.append(item);
  }
  switcher.append(current, disclosure, menu);
  els.viewSwitcher.replaceChildren(...switcher.childNodes);
  els.viewSwitcher.className = switcher.className;
}

async function loadDashboardHistory({ force = false } = {}) {
  if (state.historyLoading) return;
  const recentAttempt = state.historyLoadedAt > 0 && Date.now() - state.historyLoadedAt < HISTORY_REFRESH_MS;
  if (!force && recentAttempt) return;
  state.historyLoading = true;
  state.historyError = '';
  if (state.view === 'home' && state.stats) renderHome();
  try {
    state.history = await window.tokenMonitor.getDashboardHistory();
    state.historyLoadedAt = Date.now();
  } catch (error) {
    console.error(error);
    state.historyError = error?.message || 'Failed to load usage history';
    state.historyLoadedAt = Date.now();
  } finally {
    state.historyLoading = false;
    if (state.view === 'home' && state.stats) renderHome();
  }
}

function renderSurface() {
  const settingsOpen = state.settingsOpen;
  const detailOpen = !settingsOpen && state.view === 'session' && Boolean(state.openSession);
  els.shell.classList.toggle('settings-open', settingsOpen);
  els.shell.classList.toggle('home-mode', !settingsOpen && state.view === 'home');
  els.shell.classList.toggle('session-mode', !settingsOpen && state.view === 'session');
  els.settingsPanel.classList.toggle('hidden', !settingsOpen);
  els.settingsPanel.setAttribute('aria-hidden', String(!settingsOpen));
  els.homePanel.classList.toggle('hidden', settingsOpen || state.view !== 'home');
  els.limitsPanel.classList.toggle('hidden', settingsOpen || state.view !== 'limits');
  els.breakdown.classList.toggle('hidden', settingsOpen || detailOpen || !['tool', 'model', 'session'].includes(state.view));
  els.sessionDetailHead.classList.toggle('hidden', !detailOpen);
  els.sessionDetail.classList.toggle('hidden', !detailOpen);
  if (settingsOpen) return;
  if (state.view === 'home') renderHome();
  else if (state.view === 'limits') renderLimits();
  else if (detailOpen) renderSessionDetail();
  else renderBreakdown();
}

function render() {
  if (!state.stats) return;
  renderHeadline();
  renderViewSwitcher();
  renderSurface();
}

function mergeStatsPatch(patch, generation) {
  if (!state.stats || generation !== state.statsGeneration) return false;
  const incomingAt = Date.parse(patch?.updatedAt || '') || 0;
  const currentAt = Date.parse(state.stats.updatedAt || '') || 0;
  state.stats = {
    ...state.stats,
    ...(patch?.limits ? { limits: patch.limits } : {}),
    periods: { ...state.stats.periods, ...(patch?.periods || {}) },
    updatedAt: new Date(Math.max(incomingAt, currentAt, Date.now())).toISOString(),
  };
  return true;
}

async function loadPeriodIfMissing(period) {
  if (!['month', 'allTime'].includes(period) || state.stats?.periods?.[period]) return;
  const generation = state.statsGeneration;
  try {
    const result = await window.tokenMonitor.getPeriodStats(period);
    if (!mergeStatsPatch({ periods: { [period]: result.value }, updatedAt: result.updatedAt }, generation)) return;
    if (state.period === period) render();
  } catch (error) {
    console.error(error);
  }
}

async function refresh({ force = false } = {}) {
  if (state.refreshing) {
    state.refreshQueued = true;
    state.refreshQueuedForce ||= force;
    return;
  }
  state.refreshing = true;
  const requestPeriod = state.period;
  const generation = ++state.statsGeneration;
  els.refreshButton.classList.add('is-refreshing');
  setStatus(t('common.refreshing'));
  try {
    const nextStats = await window.tokenMonitor.getStats(statsRequestOptions(force, requestPeriod));
    if (generation !== state.statsGeneration) return;
    state.stats = nextStats;
    state.quotaLoading = false;
    if (force) state.historyLoadedAt = 0;
    const today = state.stats?.periods?.today || {};
    await window.tokenMonitor.updateTraySummary({
      todayTokens: Number(today.totalTokens) || 0,
      todayCostUsd: Number(today.costUsd) || 0,
    });
    state.lastRefreshAt = Date.now();
    setStatus();
    render();
    await syncFloatingBubbleWidth();
    els.liveDot.classList.add('pulse');
    setTimeout(() => els.liveDot.classList.remove('pulse'), 1200);
  } catch (error) {
    console.error(error);
    setStatus(error?.message || t('common.failedRefresh'), true);
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

let floatingBubbleHoverRevealTimer = null;
let floatingBubbleHoverCollapseTimer = null;
let floatingBubbleDrag = null;
let stopTrayActionListener = null;

els.settingsButton.addEventListener('click', () => {
  state.settingsOpen = !state.settingsOpen;
  state.viewMenuOpen = false;
  setPeriodMenuOpen(false);
  renderSurface();
});

els.openErrorLogsButton.addEventListener('click', async () => {
  els.openErrorLogsButton.disabled = true;
  try {
    await window.tokenMonitor.openProviderErrorLogDirectory();
  } catch (error) {
    console.error(error);
    setStatus(t('settings.errorLogsOpenFailed'), true);
  } finally {
    els.openErrorLogsButton.disabled = false;
  }
});

els.languageInput.addEventListener('change', () => {
  void saveSettings({ language: normalizeLanguage(els.languageInput.value) });
});

els.showTrayIconInput.addEventListener('change', () => {
  void saveSettings({ showTrayIcon: els.showTrayIconInput.checked });
});

els.floatingBubbleInput.addEventListener('change', () => {
  void saveSettings({ floatingBubbleEnabled: els.floatingBubbleInput.checked });
});
for (const input of els.floatingBubbleTriggerInputs) {
  input.addEventListener('change', () => {
    if (input.checked) void saveSettings({ floatingBubbleTrigger: input.value });
  });
}
els.floatingBubbleContentInput.addEventListener('change', () => {
  void saveSettings({ floatingBubbleContent: normalizeBubbleContent(els.floatingBubbleContentInput.value) });
});
els.floatingBubbleScaleInput.addEventListener('input', () => {
  els.floatingBubbleScaleValue.textContent = `${els.floatingBubbleScaleInput.value}%`;
});
els.floatingBubbleScaleInput.addEventListener('change', () => {
  void saveSettings({ floatingBubbleScale: normalizeBubbleScale(Number(els.floatingBubbleScaleInput.value) / 100) });
});
els.zoomInput.addEventListener('input', () => {
  els.zoomValue.textContent = `${els.zoomInput.value}%`;
});
els.zoomInput.addEventListener('change', () => {
  void saveSettings({ zoomFactor: normalizeZoomFactor(Number(els.zoomInput.value) / 100) });
});
els.compactTotalInput.addEventListener('change', () => {
  void saveSettings({ showCompactTotalTokens: els.compactTotalInput.checked });
});
for (const input of els.windowsBackdropInputs) {
  input.addEventListener('change', () => {
    if (input.checked) void saveSettings({ windowsBackdrop: input.value });
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

function clearBubbleTimer(name) {
  const timer = name === 'hoverReveal' ? floatingBubbleHoverRevealTimer : floatingBubbleHoverCollapseTimer;
  if (timer) clearTimeout(timer);
  if (name === 'hoverReveal') floatingBubbleHoverRevealTimer = null;
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

async function minimizeWindow() {
  try {
    await syncFloatingBubbleWidth();
    applyFloatingBubbleState(await window.tokenMonitor.minimizeMainWindow());
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
els.minButton.addEventListener('click', () => void minimizeWindow());
els.closeButton.addEventListener('click', () => appWindow.close());
els.pinButton.addEventListener('click', async () => {
  state.alwaysOnTop = !state.alwaysOnTop;
  try {
    await appWindow.setAlwaysOnTop(state.alwaysOnTop);
    els.pinButton.classList.toggle('active', state.alwaysOnTop);
    els.pinButton.title = state.alwaysOnTop ? t('window.floating') : t('window.normal');
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
  if (state.periodMenuOpen
      && !els.monthPeriodMenu?.contains(event.target)
      && !els.monthPeriodTab?.contains(event.target)) {
    setPeriodMenuOpen(false);
  }
  if (state.providerFilterMenuOpen && !event.target.closest?.('.provider-filter')) {
    state.providerFilterMenuOpen = false;
    if (['model', 'session'].includes(state.view)) renderBreakdown();
  }
});

document.addEventListener('keydown', (event) => {
  if (event.key !== 'Escape') return;
  if (state.periodMenuOpen) {
    event.preventDefault();
    setPeriodMenuOpen(false, { restoreFocus: true });
    return;
  }
  if (state.providerFilterMenuOpen) {
    event.preventDefault();
    state.providerFilterMenuOpen = false;
    renderBreakdown();
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
async function handleTrayAction(payload = {}) {
  if (payload.bubble) {
    applyFloatingBubbleState(payload.bubble);
    render();
  }
  if (payload.action === 'focus') return;
  if (payload.action === 'refresh') {
    await refresh({ force: true });
    return;
  }
  if (payload.action === 'openView' && VIEW_ORDER.includes(payload.view)) {
    state.settingsOpen = false;
    state.openSession = null;
    setView(payload.view);
    return;
  }
  if (payload.action === 'openSettings') {
    state.settingsOpen = true;
    state.viewMenuOpen = false;
    setPeriodMenuOpen(false);
    renderSurface();
  }
}

async function runBootstrapBackground(generation) {
  const quotaTask = (async () => {
    recordStartupTiming('quota-start');
    try {
      const patch = await window.tokenMonitor.getQuotaLimits();
      if (!mergeStatsPatch(patch, generation)) return;
      state.quotaLoading = false;
      render();
      await syncFloatingBubbleWidth();
      recordStartupTiming('quota-ready');
    } catch (error) {
      console.error(error);
      if (generation !== state.statsGeneration) return;
      state.quotaLoading = false;
      render();
      recordStartupTiming('quota-failed');
    }
  })();

  const slowUsageTask = (async () => {
    recordStartupTiming('slow-usage-start');
    try {
      const patch = await window.tokenMonitor.preloadSlowUsage({
        onProgress(period) {
          if (period === 'month') recordStartupTiming('month-preload-ready');
          if (period === 'allTime') recordStartupTiming('alltime-preload-ready');
        },
      });
      if (!mergeStatsPatch(patch, generation)) return;
      if (['month', 'allTime'].includes(state.period)) render();
      recordStartupTiming('slow-usage-ready');
    } catch (error) {
      console.error(error);
      recordStartupTiming('slow-usage-failed');
    }
  })();

  await Promise.allSettled([quotaTask, slowUsageTask]);
  recordStartupTiming('background-complete');
}

async function bootstrapShell() {
  try {
    stopTrayActionListener = await listen('token-lens://tray-action', ({ payload }) => {
      void handleTrayAction(payload).catch(console.error);
    });
    const [settings, bubble] = await Promise.all([
      window.tokenMonitor.getSettings(),
      window.tokenMonitor.getFloatingBubbleState(),
    ]);
    applySettings(settings);
    applyFloatingBubbleState(bubble);
    recordStartupTiming('shell-settings-ready');
  } catch (error) {
    console.error(error);
    setStatus(error?.message || t('common.failedSettings'), true);
  }

  const generation = ++state.statsGeneration;
  state.quotaLoading = true;
  setStatus(t('common.refreshing'));
  try {
    recordStartupTiming('today-scan-start');
    const bootstrapStats = await window.tokenMonitor.getBootstrapStats();
    recordStartupTiming('today-scan-ready');
    if (generation !== state.statsGeneration) return;
    state.stats = bootstrapStats;
    state.lastRefreshAt = Date.now();
    setStatus();
    render();
    recordStartupTiming('today-first-render');

    const today = state.stats?.periods?.today || {};
    void window.tokenMonitor.updateTraySummary({
      todayTokens: Number(today.totalTokens) || 0,
      todayCostUsd: Number(today.costUsd) || 0,
    }).catch(console.error);
    void syncFloatingBubbleWidth().catch(console.error);
    void runBootstrapBackground(generation);
  } catch (error) {
    console.error(error);
    state.quotaLoading = false;
    setStatus(error?.message || t('common.failedRefresh'), true);
  }
}

window.addEventListener('resize', syncPeriodIndicator);

window.addEventListener('beforeunload', () => {
  clearInterval(autoRefreshTimer);
  clearBubbleTimer('hoverReveal');
  clearBubbleTimer('hoverCollapse');
  stopTrayActionListener?.();
}, { once: true });

els.pinButton.classList.add('active');
syncPeriodTabs();
renderViewSwitcher();
void bootstrapShell();
