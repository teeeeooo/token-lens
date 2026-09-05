export const THEME_PRESETS = Object.freeze([
  Object.freeze({ id: 'default', label: 'Default', accent: '#b7ead4' }),
  Object.freeze({ id: 'obsidian', label: 'Obsidian', accent: '#e6e8ec' }),
  Object.freeze({ id: 'porcelain', label: 'Porcelain', accent: '#2563eb' }),
]);

const THEME_VALUES = Object.freeze({
  default: Object.freeze({
    '--glass-rgb': '48, 52, 56', '--overlay-rgb': '255, 255, 255', '--line-rgb': '232, 238, 244',
    '--panel-rgb': '16, 21, 30', '--sunken-rgb': '4, 8, 13', '--text': '#eef5fb', '--muted': '#a3adbb',
    '--accent': '#b7ead4', '--accent-rgb': '183, 234, 212', '--success': '#b7ead4',
    '--success-rgb': '183, 234, 212', '--number': '#f3fbf7', 'color-scheme': 'dark',
  }),
  obsidian: Object.freeze({
    '--glass-rgb': '11, 12, 14', '--overlay-rgb': '255, 255, 255', '--line-rgb': '232, 238, 244',
    '--panel-rgb': '16, 21, 30', '--sunken-rgb': '4, 8, 13', '--text': '#eceef2', '--muted': '#8f949c',
    '--accent': '#e6e8ec', '--accent-rgb': '230, 232, 236', '--success': '#b7ead4',
    '--success-rgb': '183, 234, 212', '--number': '#eceef2', 'color-scheme': 'dark',
  }),
  porcelain: Object.freeze({
    '--glass-rgb': '246, 247, 249', '--overlay-rgb': '15, 18, 24', '--line-rgb': '24, 28, 36',
    '--panel-rgb': '255, 255, 255', '--sunken-rgb': '188, 196, 206', '--text': '#1c1f26', '--muted': '#5b626d',
    '--accent': '#2563eb', '--accent-rgb': '37, 99, 235', '--success': '#18794e',
    '--success-rgb': '24, 121, 78', '--number': '#1c1f26', 'color-scheme': 'light',
  }),
});

export function normalizeThemePreset(value) {
  const id = String(value || '').trim().toLowerCase();
  return Object.prototype.hasOwnProperty.call(THEME_VALUES, id) ? id : 'default';
}

export function themeCssEntries(value) {
  const preset = normalizeThemePreset(value);
  return Object.entries(THEME_VALUES[preset]).map(([name, cssValue]) => ({ name, value: cssValue }));
}

export function applyThemePreset(root, value) {
  const preset = normalizeThemePreset(value);
  if (!root?.style) return preset;
  for (const { name, value: cssValue } of themeCssEntries(preset)) root.style.setProperty(name, cssValue);
  root.dataset.themePreset = preset;
  return preset;
}

export function normalizeZoomFactor(value) {
  const number = Number(value);
  if (!Number.isFinite(number)) return 1;
  return Math.round(Math.max(0.7, Math.min(1.6, number)) * 10) / 10;
}
