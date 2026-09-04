'use strict';

const fs = require('node:fs');
const path = require('node:path');

const root = path.join(__dirname, '..', '..');
const appPath = path.join(root, 'src', 'electron', 'renderer', 'app.js');
const homeOverviewPath = path.join(root, 'src', 'electron', 'renderer', 'homeOverview.js');
const htmlPath = path.join(root, 'src', 'electron', 'renderer', 'index.html');
const i18nPath = path.join(root, 'src', 'electron', 'renderer', 'i18n.js');
const stylesPath = path.join(root, 'src', 'electron', 'renderer', 'styles.css');
const dashboardStylesPath = path.join(root, 'src', 'electron', 'renderer', 'dashboard.css');
const fontSettingsPath = path.join(root, 'src', 'shared', 'fontSettings.js');
const trayPath = path.join(root, 'src', 'electron', 'tray.js');

function replaceExactlyOnceInSource(source, before, after, label) {
  if (source.includes(after)) return { source, changed: false };
  const first = source.indexOf(before);
  if (first < 0) throw new Error(`Token Lens renderer anchor changed upstream: ${label}`);
  if (source.indexOf(before, first + before.length) >= 0) {
    throw new Error(`Token Lens renderer anchor is ambiguous: ${label}`);
  }
  return {
    source: source.slice(0, first) + after + source.slice(first + before.length),
    changed: true
  };
}

function patchRendererProviderLists() {
  let source = fs.readFileSync(appPath, 'utf8');
  if (source.includes('const TOKEN_LENS_ALLOWED_CLIENT_IDS = new Set')) {
    console.log('Token Lens renderer provider lists are already hardened');
    return false;
  }

  const knownStart = source.indexOf('const KNOWN_CLIENTS = [');
  const limitStart = source.indexOf('const LIMIT_PROVIDERS = [', knownStart);
  const groupStart = source.indexOf('const LIMIT_PROVIDER_ACCOUNT_GROUP_IDS = {', limitStart);
  if (knownStart < 0 || limitStart < 0 || groupStart < 0) {
    throw new Error('Token Lens renderer provider-list anchors changed upstream');
  }
  if (
    source.indexOf('const KNOWN_CLIENTS = [', knownStart + 1) >= 0
    || source.indexOf('const LIMIT_PROVIDERS = [', limitStart + 1) >= 0
  ) {
    throw new Error('Token Lens renderer provider-list anchors are ambiguous');
  }

  const knownBlock = source
    .slice(knownStart, limitStart)
    .replace('const KNOWN_CLIENTS = [', 'const UPSTREAM_KNOWN_CLIENTS = [');
  const limitBlock = source
    .slice(limitStart, groupStart)
    .replace('const LIMIT_PROVIDERS = [', 'const UPSTREAM_LIMIT_PROVIDERS = [');
  const focused = [
    "const TOKEN_LENS_ALLOWED_CLIENT_IDS = new Set(['claude', 'codex', 'antigravity']);",
    knownBlock.trimEnd(),
    'const KNOWN_CLIENTS = UPSTREAM_KNOWN_CLIENTS.filter((provider) => TOKEN_LENS_ALLOWED_CLIENT_IDS.has(provider.id));',
    limitBlock.trimEnd(),
    'const LIMIT_PROVIDERS = UPSTREAM_LIMIT_PROVIDERS.filter((provider) => TOKEN_LENS_ALLOWED_CLIENT_IDS.has(provider.id));',
    ''
  ].join('\n');

  source = source.slice(0, knownStart) + focused + source.slice(groupStart);
  fs.writeFileSync(appPath, source);
  console.log('Token Lens renderer provider lists materialized');
  return true;
}

function patchCodexCreditPresentation() {
  let source = fs.readFileSync(appPath, 'utf8');
  const before = `    if (monthly) {
      const monthlyNode = limitWindowNode(monthly.label || 'Monthly', monthly, color, 0.68);
      monthlyNode.classList.add('limit-window-wide');
      windows.append(monthlyNode);
    }`;
  const after = `    if (monthly) {
      const monthlyCount = monthly.metric === 'credits'
        ? formatLimitCount(monthly, Boolean(state.settings?.showLimitUsed))
        : '';
      const monthlyDetail = monthlyCount ? \`${'${monthlyCount}'} credits\` : '';
      const monthlyNode = limitWindowNode(
        monthly.label || 'Monthly',
        monthly,
        color,
        0.68,
        null,
        monthlyDetail
      );
      monthlyNode.classList.add('limit-window-wide');
      windows.append(monthlyNode);
    }`;
  const result = replaceExactlyOnceInSource(source, before, after, 'Codex monthly credit presentation');
  if (!result.changed) {
    console.log('Token Lens Codex monthly credit presentation is already materialized');
    return false;
  }
  source = result.source;
  fs.writeFileSync(appPath, source);
  console.log('Token Lens Codex monthly credit presentation materialized');
  return true;
}

function patchUnifiedHomeQuotaPresentation() {
  let changed = false;
  let overviewSource = fs.readFileSync(homeOverviewPath, 'utf8');
  const overviewBefore = "              currency: credits ? balanceDisplay.creditsCurrency(account, window) : '',\n              resetsAt: window.resetsAt,";
  const overviewPrevious = "              currency: credits ? balanceDisplay.creditsCurrency(account, window) : '',\n              used: finiteNumber(window.used),\n              limit: finiteNumber(window.limit),\n              resetsAt: window.resetsAt,";
  const overviewAfter = "              currency: credits\n                ? balanceDisplay.creditsCurrency(account, window)\n                : String(window.currency || '').trim().toUpperCase(),\n              used: finiteNumber(window.used),\n              limit: finiteNumber(window.limit),\n              resetsAt: window.resetsAt,";
  if (!overviewSource.includes(overviewAfter)) {
    const overviewAnchor = overviewSource.includes(overviewPrevious) ? overviewPrevious : overviewBefore;
    const overviewResult = replaceExactlyOnceInSource(
      overviewSource,
      overviewAnchor,
      overviewAfter,
      'Home absolute quota metadata and currency'
    );
    if (overviewResult.changed) {
      overviewSource = overviewResult.source;
      fs.writeFileSync(homeOverviewPath, overviewSource);
      changed = true;
    }
  }

  const appSource = fs.readFileSync(appPath, 'utf8');
  const prefix = [
    'function formatHomeLimitWindowValue(window, showUsed) {',
    "  if (window?.planStatus === 'expired') return t('limits.mimo.planExpired');",
    "  if (String(window?.detail || '').toLowerCase() === 'unlimited') return t('settings.thirdparty.unlimited');"
  ].join('\n');
  const before = [prefix, '  if (isCreditsWindow(window)) {'].join('\n');
  const previous = [
    prefix,
    '  if (',
    '    isCreditsWindow(window)',
    "    && String(window?.currency || '').trim().toUpperCase() === 'CREDITS'",
    '    && optionalFiniteNumber(window?.limit) !== null',
    '  ) {',
    '    const percent = limitFillPercent(window?.remainingPercent, window?.usedPercent, showUsed);',
    "    const percentage = formatPercent(percent) + ' ' + limitModeSuffix(showUsed);",
    '    const count = formatLimitCount(window, showUsed);',
    "    return count ? percentage + ' · ' + count + ' credits' : percentage;",
    '  }',
    '  if (isCreditsWindow(window)) {'
  ].join('\n');
  const after = [
    prefix,
    '  const quotaUsed = optionalFiniteNumber(window?.used);',
    '  const quotaLimit = optionalFiniteNumber(window?.limit);',
    "  const quotaCurrency = String(window?.currency || '').trim().toUpperCase();",
    '  if (quotaUsed !== null && quotaLimit !== null && quotaLimit > 0 && quotaCurrency) {',
    '    const percent = limitFillPercent(window?.remainingPercent, window?.usedPercent, showUsed);',
    "    const percentage = formatPercent(percent) + ' ' + limitModeSuffix(showUsed);",
    "    const count = quotaCurrency === 'CREDITS' ? formatLimitCount(window, showUsed) : '';",
    "    const absolute = quotaCurrency === 'CREDITS'",
    "      ? (count ? count + ' credits' : '')",
    "      : formatMoney(showUsed ? Math.max(0, quotaUsed) : Math.max(0, quotaLimit - quotaUsed), quotaCurrency) + '/' + formatMoney(quotaLimit, quotaCurrency);",
    "    return absolute ? percentage + ' · ' + absolute : percentage;",
    '  }',
    '  if (isCreditsWindow(window)) {'
  ].join('\n');
  if (!appSource.includes(after)) {
    const appAnchor = appSource.includes(previous) ? previous : before;
    const appResult = replaceExactlyOnceInSource(appSource, appAnchor, after, 'Home capability-based absolute quota formatter');
    if (appResult.changed) {
      fs.writeFileSync(appPath, appResult.source);
      changed = true;
    }
  }
  console.log(changed ? 'Token Lens unified Home quota presentation materialized' : 'Token Lens unified Home quota presentation is already materialized');
  return changed;
}

function patchRendererHtml() {
  let source = fs.readFileSync(htmlPath, 'utf8');
  let changed = false;

  function replaceOnce(before, after, label) {
    if (source.includes(after)) return;
    const first = source.indexOf(before);
    if (first < 0) throw new Error(`Token Lens renderer HTML anchor changed upstream: ${label}`);
    if (source.indexOf(before, first + before.length) >= 0) {
      throw new Error(`Token Lens renderer HTML anchor is ambiguous: ${label}`);
    }
    source = source.slice(0, first) + after + source.slice(first + before.length);
    changed = true;
  }

  replaceOnce('<title>Token Monitor</title>', '<title>Token Lens</title>', 'document title');
  replaceOnce(
    '<span class="app-title-text">Token Monitor</span>',
    '<span class="app-title-text">Token Lens</span>',
    'visible application title'
  );
  replaceOnce(
    '<span class="app-title-measure" aria-hidden="true">Token Monitor</span>',
    '<span class="app-title-measure" aria-hidden="true">Token Lens</span>',
    'application title measurement'
  );

  const stylesheet = '<link rel="stylesheet" href="styles.css" />';
  const policyStyle = `${stylesheet}\n    <style id="token-lens-downstream-ui-policy">\n      /* Token Lens is a focused monitor, not an account/update/sync manager. */\n      .app-update-settings,\n      .settings-sync-group,\n      #tokscaleGroup,\n      .settings-subgroup:has(#discordRpcInput),\n      #claudeAccountGroup,\n      #codexAccountGroup,\n      #antigravityAccountGroup,\n      #cursorAccountGroup,\n      #opencodeCookieGroup,\n      #kimiAccountGroup,\n      #zedAccountGroup,\n      #copilotAccountGroup,\n      #mimoAccountGroup,\n      #zaiAccountGroup,\n      #zaiteamAccountGroup,\n      #deepseekAccountGroup,\n      #openrouterAccountGroup,\n      #minimaxAccountGroup,\n      #volcengineAccountGroup,\n      #qoderAccountGroup,\n      #traeAccountGroup,\n      #commandcodeAccountGroup,\n      #ollamaAccountGroup,\n      #thirdpartyAccountGroup {\n        display: none !important;\n      }\n    </style>`;
  replaceOnce(stylesheet, policyStyle, 'downstream UI policy stylesheet');

  if (changed) {
    fs.writeFileSync(htmlPath, source);
    console.log('Token Lens renderer HTML materialized');
  } else {
    console.log('Token Lens renderer HTML is already hardened');
  }
  return changed;
}

function patchDefaultInterfaceFont() {
  let source = fs.readFileSync(fontSettingsPath, 'utf8');
  const before = `  const DEFAULT_INTERFACE_FONT = 'ui-monospace, SFMono-Regular, Menlo, Consolas, "Liberation Mono", monospace';
  const DEFAULT_DASHBOARD_INTERFACE_FONT = '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif';
  const DEFAULT_DISPLAY_FONT = '-apple-system, BlinkMacSystemFont, "SF Pro Display", "Segoe UI", sans-serif';
  const SYSTEM_UI_FONT = 'system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif';
  const FONT_PRESETS = Object.freeze({
    app: '',
    system: SYSTEM_UI_FONT,
    mono: DEFAULT_INTERFACE_FONT
  });`;
  const after = `  const MONOSPACE_INTERFACE_FONT = 'ui-monospace, SFMono-Regular, Menlo, Consolas, "Liberation Mono", monospace';
  const SYSTEM_UI_FONT = 'system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif';
  const DEFAULT_INTERFACE_FONT = SYSTEM_UI_FONT;
  const DEFAULT_DASHBOARD_INTERFACE_FONT = '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif';
  const DEFAULT_DISPLAY_FONT = '-apple-system, BlinkMacSystemFont, "SF Pro Display", "Segoe UI", sans-serif';
  const FONT_PRESETS = Object.freeze({
    app: '',
    system: SYSTEM_UI_FONT,
    mono: MONOSPACE_INTERFACE_FONT
  });`;
  const result = replaceExactlyOnceInSource(source, before, after, 'default System interface font');
  if (!result.changed) {
    console.log('Token Lens default System interface font is already materialized');
    return false;
  }
  source = result.source;
  fs.writeFileSync(fontSettingsPath, source);
  console.log('Token Lens default System interface font materialized');
  return true;
}

function patchReadableFontSizes() {
  const rootMarker = '  --token-lens-font-size-body: 12px;\n  --token-lens-font-size-small: 11px;';
  let styles = fs.readFileSync(stylesPath, 'utf8');
  let changed = false;
  if (!styles.includes(rootMarker)) {
    const anchor = ':root {\n';
    const first = styles.indexOf(anchor);
    if (first < 0 || styles.indexOf(anchor, first + anchor.length) >= 0) {
      throw new Error('Token Lens typography root anchor changed or is ambiguous upstream');
    }
    styles = styles.slice(0, first + anchor.length) + rootMarker + '\n' + styles.slice(first + anchor.length);
    changed = true;
  }

  function promote(pathName, source) {
    const promoted = source
      .replace(/font-size:\s*11px;/g, 'font-size: var(--token-lens-font-size-body);')
      .replace(/font-size:\s*10px;/g, 'font-size: var(--token-lens-font-size-small);');
    if (promoted !== source) changed = true;
    if (/font-size:\s*(?:10|11)px;/.test(promoted)) {
      throw new Error(`Token Lens typography promotion incomplete: ${pathName}`);
    }
    return promoted;
  }

  styles = promote('styles.css', styles);
  let dashboardStyles = fs.readFileSync(dashboardStylesPath, 'utf8');
  dashboardStyles = promote('dashboard.css', dashboardStyles);

  if (changed) {
    fs.writeFileSync(stylesPath, styles);
    fs.writeFileSync(dashboardStylesPath, dashboardStyles);
    console.log('Token Lens readable font sizes materialized');
  } else {
    console.log('Token Lens readable font sizes are already materialized');
  }
  return changed;
}

function patchVisibleProductBrand(targetPath, label) {
  const source = fs.readFileSync(targetPath, 'utf8');
  const branded = source.replaceAll('Token Monitor', 'Token Lens');
  if (branded.includes('Token Monitor')) {
    throw new Error(`Token Lens branding cleanup failed: ${label}`);
  }
  if (branded === source) {
    console.log(`Token Lens ${label} branding is already materialized`);
    return false;
  }
  fs.writeFileSync(targetPath, branded);
  console.log(`Token Lens ${label} branding materialized`);
  return true;
}

patchRendererProviderLists();
patchCodexCreditPresentation();
patchUnifiedHomeQuotaPresentation();
patchRendererHtml();
patchDefaultInterfaceFont();
patchReadableFontSizes();
patchVisibleProductBrand(htmlPath, 'renderer HTML');
patchVisibleProductBrand(i18nPath, 'renderer translations');
patchVisibleProductBrand(trayPath, 'tray surface');
