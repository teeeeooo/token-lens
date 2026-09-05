const CLIENT_LABELS = Object.freeze({
  claude: 'Claude Code',
  codex: 'Codex',
  gemini: 'Gemini CLI',
  antigravity: 'Antigravity',
});

const CLIENT_COLORS = Object.freeze({
  claude: '#cc7c5e',
  codex: '#49a3b0',
  gemini: '#4285f4',
  antigravity: '#4285f4',
  default: '#6ab4f0',
});

const FALLBACK_MODEL_COLORS = ['#6ab4f0', '#5fbf8a', '#a57df0', '#d97bc4', '#f0d66a', '#f06a7b'];
const PROVIDER_ORDER = ['codex', 'claude', 'gemini', 'antigravity'];

function finite(value) {
  const number = Number(value);
  return Number.isFinite(number) ? number : 0;
}

function optionalFinite(value) {
  if (value === null || value === undefined || value === '') return null;
  const number = Number(value);
  return Number.isFinite(number) ? number : null;
}

function normalizedId(value) {
  return String(value || '').trim().toLowerCase();
}
export function formatNumber(value) {
  return Math.round(finite(value)).toLocaleString('en-US');
}

export function formatCompact(value) {
  const number = Math.max(0, finite(value));
  if (number < 1_000) return formatNumber(number);
  if (number < 1_000_000) return `${(number / 1_000).toFixed(number >= 100_000 ? 0 : 1)}K`;
  if (number < 1_000_000_000) return `${(number / 1_000_000).toFixed(number >= 100_000_000 ? 0 : 1)}M`;
  return `${(number / 1_000_000_000).toFixed(number >= 100_000_000_000 ? 0 : 1)}B`;
}

export function formatCost(value) {
  return `$${Math.max(0, finite(value)).toFixed(2)}`;
}

export function formatPercent(value) {
  const number = Math.max(0, Math.min(100, finite(value)));
  return `${number.toFixed(number < 10 && number % 1 ? 1 : 0)}%`;
}

export function clientLabel(client) {
  const id = normalizedId(client);
  return CLIENT_LABELS[id] || id || 'Unknown';
}
export function clientColor(client) {
  return CLIENT_COLORS[normalizedId(client)] || CLIENT_COLORS.default;
}

export function modelVendorFor(model) {
  const name = normalizedId(model);
  if (/claude|anthropic|sonnet|opus|haiku/.test(name)) return 'claude';
  if (/gpt|openai|codex|^o[134](?:-|$)|chatgpt/.test(name)) return 'codex';
  if (/gemini|gemma|google/.test(name)) return 'gemini';
  return null;
}

export function modelColor(model) {
  const vendor = modelVendorFor(model);
  if (vendor) return clientColor(vendor);
  const name = normalizedId(model);
  let hash = 0;
  for (const char of name) hash = (hash * 31 + char.charCodeAt(0)) | 0;
  return FALLBACK_MODEL_COLORS[Math.abs(hash) % FALLBACK_MODEL_COLORS.length];
}

export function iconClassForClient(client) {
  const id = normalizedId(client);
  return ['claude', 'codex', 'gemini', 'antigravity'].includes(id) ? `row-icon-${id}` : 'row-icon-token-monitor';
}
export function modelRows(period) {
  const total = Math.max(0, finite(period?.totalTokens));
  return Object.entries(period?.models || {})
    .map(([model, tokens]) => ({
      key: model,
      name: model,
      value: finite(tokens),
      cost: finite(period?.modelCosts?.[model]),
      share: total > 0 ? finite(tokens) / total : 0,
      color: modelColor(model),
      iconClass: `row-icon-${modelVendorFor(model) || 'token-monitor'}`,
    }))
    .filter((row) => row.value > 0)
    .sort((a, b) => b.value - a.value || b.cost - a.cost || a.name.localeCompare(b.name));
}

export function toolRows(period) {
  const total = Math.max(0, finite(period?.totalTokens));
  return Object.entries(period?.clients || {})
    .map(([client, tokens]) => ({
      key: client,
      name: clientLabel(client),
      value: finite(tokens),
      cost: finite(period?.clientCosts?.[client]),
      share: total > 0 ? finite(tokens) / total : 0,
      color: clientColor(client),
      iconClass: iconClassForClient(client),
    }))
    .filter((row) => row.value > 0)
    .sort((a, b) => b.value - a.value || b.cost - a.cost || a.name.localeCompare(b.name));
}

export function sessionRows(period) {
  return Object.entries(period?.sessions || {})
    .map(([key, session]) => {
      const models = Object.entries(session?.models || {})
        .filter(([, tokens]) => finite(tokens) > 0)
        .sort((a, b) => finite(b[1]) - finite(a[1]));
      const model = models[0]?.[0] || '';
      const messages = Math.max(0, Math.round(finite(session?.messageCount)));
      const context = [clientLabel(session?.client), model].filter(Boolean).join(' · ');
      const sessionTitle = String(session?.sessionTitle || '').trim();
      const projectLabel = String(session?.projectLabel || '').trim();
      const sessionId = String(session?.sessionId || '').trim();
      const name = sessionTitle || projectLabel || sessionId || context || 'Session';
      const detailParts = [];
      if (context && name !== context) detailParts.push(context);
      if (messages > 0) detailParts.push(`${formatNumber(messages)} msg${messages === 1 ? '' : 's'}`);
      return {
        key,
        name,
        detail: detailParts.join(' · '),
        value: finite(session?.totalTokens),
        cost: finite(session?.costUsd),
        color: clientColor(session?.client),
        iconClass: iconClassForClient(session?.client),
        client: normalizedId(session?.client),
        sessionId,
      };
    })
    .filter((row) => row.value > 0)
    .sort((a, b) => b.value - a.value || b.cost - a.cost || a.name.localeCompare(b.name));
}

function windowPercent(window) {
  const remaining = Number(window?.remainingPercent);
  if (Number.isFinite(remaining)) return Math.max(0, Math.min(100, remaining));
  const used = Number(window?.usedPercent);
  return Number.isFinite(used) ? Math.max(0, Math.min(100, 100 - used)) : null;
}

function providerRank(id) {
  const index = PROVIDER_ORDER.indexOf(id);
  return index < 0 ? PROVIDER_ORDER.length : index;
}

export function quotaRows(limits) {
  const byProvider = new Map((limits?.providers || []).map((provider) => [normalizedId(provider?.provider), provider]));
  return PROVIDER_ORDER.map((id) => {
    const provider = byProvider.get(id);
    return {
      key: id,
      providerId: id,
      name: clientLabel(id).replace(' Code', ''),
      plan: String(provider?.planLabel || ''),
      status: provider?.status || 'unavailable',
      color: clientColor(id),
      iconClass: iconClassForClient(id),
      windows: (provider?.windows || []).map((window) => ({
        kind: window.kind || 'additional',
        metric: String(window.metric || 'quota'),
        label: String(window.label || ''),
        additional: window.additional === true,
        used: optionalFinite(window.used),
        limit: optionalFinite(window.limit),
        remaining: optionalFinite(window.remaining),
        usedPercent: optionalFinite(window.usedPercent),
        remainingPercent: windowPercent(window),
        resetsAt: window.resetsAt || null,
        currency: String(window.currency || '').trim().toUpperCase(),
        showMeter: window.showMeter !== false,
        source: String(window.source || ''),
      })),
      resetCredits: provider?.resetCredits || null,
    };
  }).sort((a, b) => providerRank(a.providerId) - providerRank(b.providerId));
}

export function formatQuotaCount(window, showUsed = false) {
  const used = optionalFinite(window?.used);
  const limit = optionalFinite(window?.limit);
  if (used === null || limit === null || limit <= 0) return '';
  const trim = (value) => Number(Math.max(0, value).toFixed(2)).toString();
  return `${trim(showUsed ? used : limit - used)}/${trim(limit)}`;
}


export function homeQuotaWindows(row) {
  const windows = Array.isArray(row?.windows) ? row.windows.filter((window) => window?.remainingPercent != null) : [];
  const constrained = (items, count = 2) => items
    .slice()
    .sort((a, b) => a.remainingPercent - b.remainingPercent || a.label.localeCompare(b.label))
    .slice(0, count);
  if (row?.providerId === 'gemini') return constrained(windows);
  if (row?.providerId === 'antigravity') {
    const canonical = windows.filter((window) => !window.additional && ['session', 'weekly'].includes(window.kind));
    if (canonical.length) {
      return ['session', 'weekly'].flatMap((kind) => constrained(canonical.filter((window) => window.kind === kind), 1));
    }
    return constrained(windows);
  }
  return windows.filter((window) => !window.additional && ['session', 'weekly'].includes(window.kind)).slice(0, 2);
}

export function quotaWindowLabel(window) {
  const label = String(window?.label || '').trim();
  const normalized = label.toLowerCase();
  if (window?.additional) return label || 'Additional';
  if (window?.kind === 'session') {
    if (label && !['5h', '5 hr', '5hr', '5-hour', 'session'].includes(normalized)) return label;
    return '5-hour';
  }
  if (window?.kind === 'weekly') {
    if (label && !['weekly', 'week'].includes(normalized)) return label;
    return 'Weekly';
  }
  if (window?.kind === 'billing') return label || 'Monthly';
  return label || 'Quota';
}

export function formatResetTime(value, now = new Date()) {
  const date = value ? new Date(value) : null;
  if (!date || Number.isNaN(date.getTime())) return '';
  const sameDay = date.getFullYear() === now.getFullYear()
    && date.getMonth() === now.getMonth()
    && date.getDate() === now.getDate();
  const time = date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  if (sameDay) return `Resets ${time}`;
  const day = date.toLocaleDateString([], { month: 'short', day: 'numeric' });
  return `Resets ${day} ${time}`;
}
