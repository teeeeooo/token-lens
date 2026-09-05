import { rangeForSelection } from './fixed-periods.js';

function finite(value) {
  const number = Number(value);
  return Number.isFinite(number) ? number : 0;
}

function pad2(value) {
  return String(value).padStart(2, '0');
}

function compactTime(value, now = new Date()) {
  const date = value ? new Date(value) : null;
  if (!date || Number.isNaN(date.getTime())) return '';
  const time = `${pad2(date.getHours())}:${pad2(date.getMinutes())}`;
  const sameDay = date.getFullYear() === now.getFullYear()
    && date.getMonth() === now.getMonth()
    && date.getDate() === now.getDate();
  return sameDay ? time : `${pad2(date.getMonth() + 1)}/${pad2(date.getDate())} ${time}`;
}

export function formatToolList(tools) {
  return Array.from(new Set((tools || []).map((value) => String(value || '').trim()).filter(Boolean))).join(' · ');
}

function turnRows(turns) {
  return (turns || []).map((turn, index) => ({
    key: `turn:${index}`,
    label: `Reply #${index + 1}`,
    value: finite(turn?.tokens?.total),
    cost: finite(turn?.costEstimate),
    tokens: turn?.tokens || {},
    tools: formatToolList(turn?.tools),
  }));
}

export function exchangeRows(detail, options = {}) {
  const now = options.now || new Date();
  const sortBy = options.sortBy === 'tokens' ? 'tokens' : 'time';
  const rows = (detail?.exchanges || []).map((exchange, index) => {
    const turnCount = Math.max(0, Math.round(finite(exchange?.turnCount)));
    const tools = Array.isArray(exchange?.tools) ? exchange.tools : [];
    return {
      key: `exchange:${index}`,
      title: `Exchange #${index + 1}`,
      subtitle: [
        compactTime(exchange?.startedAt, now),
        `${turnCount} turn${turnCount === 1 ? '' : 's'}`,
        tools.length ? `${tools.length} tool${tools.length === 1 ? '' : 's'}` : '',
      ].filter(Boolean).join(' · '),
      value: finite(exchange?.tokens?.total),
      cost: finite(exchange?.costEstimate),
      startTime: new Date(exchange?.startedAt || '').getTime() || 0,
      turns: turnRows(exchange?.turns),
    };
  });
  if (sortBy === 'tokens') rows.sort((a, b) => b.value - a.value || b.startTime - a.startTime);
  else rows.sort((a, b) => b.startTime - a.startTime || b.value - a.value);
  return rows;
}

function localStartFromKey(key) {
  const match = String(key || '').match(/^(\d{4})-(\d{2})-(\d{2})$/);
  if (!match) return null;
  const date = new Date(Number(match[1]), Number(match[2]) - 1, Number(match[3]));
  const value = date.getTime();
  return Number.isFinite(value) ? value : null;
}

export function periodStartTimeMs(selection, options = {}) {
  const now = options.now instanceof Date ? options.now : new Date(options.now || Date.now());
  if (Number.isNaN(now.getTime()) || selection === 'allTime') return null;
  if (selection === 'today') return new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  if (selection === 'month') return new Date(now.getFullYear(), now.getMonth(), 1).getTime();
  const range = rangeForSelection(selection, { now, locale: options.locale });
  return range ? localStartFromKey(range.start) : null;
}
