import { quotaRows } from './renderer-model.js';

export const BUBBLE_CONTENT_MODES = Object.freeze([
  'limitsAllSessions',
  'icon',
  'barsSession',
  'barsWeekly',
  'barsAllSessions',
  'bars',
]);

export function normalizeBubbleContent(value) {
  return BUBBLE_CONTENT_MODES.includes(value) ? value : 'limitsAllSessions';
}

export function bubblePercentLabel(value) {
  if (value === null || value === undefined || value === '') return '';
  const number = Number(value);
  if (!Number.isFinite(number)) return '';
  return `${Math.round(Math.max(0, Math.min(100, number)))}%`;
}

function meteredWindows(row, kind = '') {
  if (!row || row.status !== 'ok') return [];
  return (row.windows || []).filter((window) => (
    window
    && window.showMeter !== false
    && (!kind || window.kind === kind)
    && !(row.providerId === 'codex' && window.additional)
    && window.remainingPercent !== null
    && window.remainingPercent !== undefined
    && window.remainingPercent !== ''
    && Number.isFinite(Number(window.remainingPercent))
  ));
}

function preferredWindow(row, kind) {
  const windows = meteredWindows(row, kind);
  if (windows.length < 2) return windows[0] || null;
  const canonicalLabels = kind === 'weekly' ? new Set(['', 'weekly'])
    : kind === 'billing' ? new Set(['', 'total'])
      : new Set(['']);
  const canonical = windows.find((window) => canonicalLabels.has(String(window.label || '').trim().toLowerCase()));
  if (canonical) return canonical;
  return windows.reduce((pick, window) => (
    !pick || window.remainingPercent < pick.remainingPercent ? window : pick
  ), null);
}

function compactSelection(row) {
  const session = preferredWindow(row, 'session');
  const daily = preferredWindow(row, 'daily');
  const weekly = preferredWindow(row, 'weekly');
  const billing = preferredWindow(row, 'billing');
  const other = preferredWindow(row, 'other');
  const primaryWindow = session || daily || weekly || billing || other;
  if (!primaryWindow) return null;
  const secondaryWindow = session ? (daily || weekly) : daily ? weekly : null;
  return {
    providerId: row.providerId,
    iconClass: row.iconClass,
    color: row.color,
    primaryWindow,
    secondaryWindow,
    primaryPercent: primaryWindow.remainingPercent,
    secondaryPercent: secondaryWindow?.remainingPercent ?? null,
  };
}

export function configuredBubbleSelections(limits) {
  return quotaRows(limits).map(compactSelection).filter(Boolean).slice(0, 2);
}

function pickWorst(limits, kind = '') {
  let worst = null;
  for (const row of quotaRows(limits)) {
    const selection = compactSelection(row);
    if (!selection) continue;
    const selectedWindow = kind
      ? preferredWindow(row, kind)
      : [selection.primaryWindow, selection.secondaryWindow].filter(Boolean)
        .reduce((pick, window) => (!pick || window.remainingPercent < pick.remainingPercent ? window : pick), null);
    if (!selectedWindow) continue;
    const remaining = Number(selectedWindow.remainingPercent);
    if (!worst || remaining < worst.remaining) worst = { ...selection, selectedWindow, remaining };
  }
  return worst;
}

function pickByPriority(limits, kinds) {
  for (const kind of kinds) {
    const pick = pickWorst(limits, kind);
    if (pick) return pick;
  }
  return null;
}

function providerBars(selection) {
  if (!selection) return { kind: 'icon' };
  return {
    kind: 'providerBars',
    providerId: selection.providerId,
    iconClass: selection.iconClass,
    color: selection.color,
    primaryPercent: selection.primaryPercent,
    secondaryPercent: selection.secondaryPercent,
  };
}

export function floatingBubbleModel(limits, rawMode) {
  const mode = normalizeBubbleContent(rawMode);
  if (mode === 'icon') return { kind: 'icon' };
  const configured = configuredBubbleSelections(limits);
  if (mode === 'limitsAllSessions') {
    if (!configured.length) return { kind: 'icon' };
    const entries = configured.length === 1
      ? [{ ...configured[0], percents: [configured[0].primaryPercent, configured[0].secondaryPercent].filter((value) => value != null) }]
      : configured.map((selection) => ({ ...selection, percents: [selection.primaryPercent] }));
    return { kind: 'limits', entries };
  }
  if (mode === 'barsAllSessions') {
    if (!configured.length) return { kind: 'icon' };
    if (configured.length === 1) return providerBars(configured[0]);
    return { kind: 'barsPair', percents: configured.map((selection) => selection.primaryPercent) };
  }
  if (mode === 'barsSession') return providerBars(pickByPriority(limits, ['session', 'weekly']));
  if (mode === 'barsWeekly') return providerBars(pickWorst(limits, 'weekly'));
  return providerBars(pickWorst(limits));
}
