function number(value) {
  const n = Number(value);
  return Number.isFinite(n) ? n : 0;
}

export function localDayKey(date = new Date()) {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, '0');
  const day = String(date.getDate()).padStart(2, '0');
  return `${year}-${month}-${day}`;
}

export function historySinceKey(now = new Date(), days = 365) {
  const count = Math.max(1, Math.trunc(number(days)) || 365);
  const start = new Date(now.getFullYear(), now.getMonth(), now.getDate() - (count - 1));
  return localDayKey(start);
}

function addDaysUtc(key, delta) {
  return new Date(Date.parse(`${key}T00:00:00Z`) + delta * 86400000).toISOString().slice(0, 10);
}

function sundayIndex(key) {
  return new Date(Date.parse(`${key}T00:00:00Z`)).getUTCDay();
}

export function patchToday(daily = [], todayPeriod = {}, now = new Date()) {
  const date = localDayKey(now);
  const rows = (Array.isArray(daily) ? daily : []).map((row) => ({ ...row }));
  const next = {
    date,
    tokens: Math.max(0, number(todayPeriod?.totalTokens)),
    cost: Math.max(0, number(todayPeriod?.costUsd)),
  };
  const index = rows.findIndex((row) => String(row?.date || '').slice(0, 10) === date);
  if (index >= 0) rows[index] = { ...rows[index], ...next };
  else rows.push(next);
  return rows.sort((a, b) => String(a.date).localeCompare(String(b.date)));
}

export function heatIntensityRows(daily = []) {
  const rows = Array.isArray(daily) ? daily : [];
  const max = Math.max(0, ...rows.map((row) => number(row?.tokens)));
  return rows.map((row) => {
    const ratio = max > 0 ? number(row?.tokens) / max : 0;
    const intensity = ratio >= 0.75 ? 4 : ratio >= 0.5 ? 3 : ratio >= 0.25 ? 2 : ratio > 0 ? 1 : 0;
    return { ...row, intensity };
  });
}

export function rollingHeatmap(daily = [], { endDate, days = 365, cell = 9, gap = 3 } = {}) {
  const end = String(endDate || localDayKey()).slice(0, 10);
  const startDate = addDaysUtc(end, -(Math.max(1, Math.trunc(number(days)) || 365) - 1));
  const alignedStart = addDaysUtc(startDate, -sundayIndex(startDate));
  const byDate = new Map(heatIntensityRows(daily).map((row) => [String(row.date).slice(0, 10), row]));
  const cells = [];
  const monthLabels = [];
  let col = 0;
  for (let key = alignedStart; key <= end; key = addDaysUtc(key, 1)) {
    const offsetDays = Math.round((Date.parse(`${key}T00:00:00Z`) - Date.parse(`${alignedStart}T00:00:00Z`)) / 86400000);
    col = Math.floor(offsetDays / 7);
    const row = sundayIndex(key);
    const value = byDate.get(key) || {};
    if (key.slice(8, 10) === '01') monthLabels.push({ col, date: key });
    cells.push({
      date: key, col, row, x: col * (cell + gap), y: row * (cell + gap) + 13,
      size: cell, intensity: number(value.intensity), tokens: number(value.tokens), cost: number(value.cost),
    });
  }
  const width = cells.length ? (col + 1) * (cell + gap) - gap : 0;
  return { cells, monthLabels, width, height: 7 * (cell + gap) - gap + 13, cell, gap };
}

export function trendModel(daily = [], { limit = 45, width = 300, height = 70 } = {}) {
  const rows = (Array.isArray(daily) ? daily : []).slice(-Math.max(1, Math.trunc(number(limit)) || 45));
  const max = Math.max(1, ...rows.map((row) => number(row?.tokens)));
  const innerHeight = Math.max(1, height - 8);
  const x = (index) => rows.length <= 1 ? width / 2 : (index / (rows.length - 1)) * width;
  const y = (value) => 4 + innerHeight - (number(value) / max) * innerHeight;
  const points = rows.map((row, index) => ({ x: x(index), y: y(row?.tokens), row }));
  const line = points.map((point, index) => `${index ? 'L' : 'M'}${point.x.toFixed(2)} ${point.y.toFixed(2)}`).join(' ');
  const area = points.length
    ? `${line} L${points[points.length - 1].x.toFixed(2)} ${height} L${points[0].x.toFixed(2)} ${height} Z`
    : '';
  const dates = rows.length
    ? [rows[0]?.date || '', rows[Math.floor((rows.length - 1) / 2)]?.date || '', rows[rows.length - 1]?.date || '']
    : [];
  return { rows, max, width, height, line, area, dates };
}

export function historyViewModel(history, todayPeriod, now = new Date()) {
  const daily = patchToday(history?.daily || [], todayPeriod, now);
  return {
    daily,
    heatmap: rollingHeatmap(daily, { endDate: localDayKey(now) }),
    trend: trendModel(daily),
    activeDays: Math.max(0, number(history?.summary?.activeDays)),
    peakDayTokens: Math.max(0, ...daily.map((row) => number(row?.tokens))),
  };
}
