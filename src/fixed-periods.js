const MONTH_MODES = Object.freeze(['month', 'week', 'last7', 'last30']);
const LABELS = Object.freeze({
  month: 'MONTH',
  week: 'WEEK',
  last7: '7D',
  last30: '30D',
});

function normalizeDateKey(value) {
  const text = String(value || '').trim();
  return /^\d{4}-\d{2}-\d{2}$/.test(text) ? text : '';
}

export function localDayKey(value = new Date()) {
  const date = value instanceof Date ? value : new Date(value);
  if (Number.isNaN(date.getTime())) return '';
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, '0');
  const day = String(date.getDate()).padStart(2, '0');
  return `${year}-${month}-${day}`;
}

export function dayKeyAddDays(value, delta) {
  const key = normalizeDateKey(value);
  if (!key) return '';
  const date = new Date(`${key}T00:00:00Z`);
  date.setUTCDate(date.getUTCDate() + Number(delta || 0));
  return date.toISOString().slice(0, 10);
}
export function weekStartsOn(locale) {
  try {
    const resolved = new Intl.Locale(String(locale || 'en'));
    const info = typeof resolved.getWeekInfo === 'function' ? resolved.getWeekInfo() : resolved.weekInfo;
    const firstDay = Number(info?.firstDay);
    if (Number.isInteger(firstDay) && firstDay >= 1 && firstDay <= 7) return firstDay % 7;
  } catch (_) {
    // Fall back to ISO Monday.
  }
  return 1;
}

export function normalizeMonthMode(value) {
  return MONTH_MODES.includes(value) ? value : 'month';
}

export function slotForSelection(value) {
  return MONTH_MODES.includes(value) ? 'month' : value;
}

export function displayLabel(value) {
  return LABELS[normalizeMonthMode(value)];
}

export function rangeForSelection(selection, options = {}) {
  const todayKey = normalizeDateKey(options.todayKey) || localDayKey(options.now);
  if (!todayKey) return null;
  if (selection === 'week') {
    const weekday = new Date(`${todayKey}T00:00:00Z`).getUTCDay();
    const offset = (weekday - weekStartsOn(options.locale) + 7) % 7;
    return { start: dayKeyAddDays(todayKey, -offset), end: todayKey };
  }
  if (selection === 'last7') return { start: dayKeyAddDays(todayKey, -6), end: todayKey };
  if (selection === 'last30') return { start: dayKeyAddDays(todayKey, -29), end: todayKey };
  return null;
}

export function derivedRequest(selection, options = {}) {
  const range = rangeForSelection(selection, options);
  if (!range) return null;
  return { key: normalizeMonthMode(selection), since: range.start };
}

export function periodMenuTargetIndex(key, currentIndex, itemCount) {
  const count = Math.max(0, Math.floor(Number(itemCount) || 0));
  if (count === 0) return -1;
  const current = Math.max(0, Math.min(count - 1, Math.floor(Number(currentIndex) || 0)));
  if (key === 'ArrowDown') return (current + 1) % count;
  if (key === 'ArrowUp') return (current - 1 + count) % count;
  if (key === 'Home') return 0;
  if (key === 'End') return count - 1;
  return -1;
}
