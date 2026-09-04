'use strict';

const fs = require('node:fs');
const path = require('node:path');

const root = path.join(__dirname, '..', '..');
const appPath = path.join(root, 'src', 'electron', 'renderer', 'app.js');
const htmlPath = path.join(root, 'src', 'electron', 'renderer', 'index.html');
const i18nPath = path.join(root, 'src', 'electron', 'renderer', 'i18n.js');
const trayPath = path.join(root, 'src', 'electron', 'tray.js');

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
patchRendererHtml();
patchVisibleProductBrand(htmlPath, 'renderer HTML');
patchVisibleProductBrand(i18nPath, 'renderer translations');
patchVisibleProductBrand(trayPath, 'tray surface');
