# v1 → v2 Porting Map

## Purpose

This file controls reuse from the Electron-based Token Lens v1 codebase. It prevents v2 work from treating the entire v1 tree as migration scope.

Reference lineage at v2 bootstrap:

- v1 branch: `v1-legacy`
- v1 reference commit: `493d4a5687a767af079fb30ee3786ebff745ce1a`
- original upstream project: `Javis603/token-monitor`

Use the exact commit above for archaeology when later `main` movement would make a comparison ambiguous.

## Port / preserve deliberately

### Renderer and UX

Preserve the established product behavior and visual design, including:

- main dashboard and limits presentation;
- usage charts and breakdown interactions;
- model/session exploration;
- theme, typography, icons, spacing, and interaction conventions;
- tray behavior;
- floating-bubble behavior;
- dashboard window behavior.

Port renderer assets selectively; remove code paths for features excluded from v2 rather than recreating their backend APIs. Home Limits keeps v1 compact behavior by hiding providers with no usable quota window; unavailable providers remain visible only in the dedicated Limits view. Period controls and window actions must use separate titlebar regions rather than the v1 overlapping hover target, with a symmetric titlebar keeping DAY/MONTH/TOTAL truly centered and clipping its moving indicator inside the selector. Preserve v1's last-normal-window bounds persistence, but use the roomier v2 first-launch default of 380x720 and a 300px expanded minimum width so the retained Home/Trend surface and separated titlebar do not start clipped.

Appearance is a deliberate subset rather than full v1 settings parity. Retain built-in theme presets, zoom, compact total-token display, OS-driven reduced-motion behavior, always-on live/tool indicators, the fixed system UI font stack, `Auto (system)` interface language with the retained v1 locales, and a Windows-only `Off` / `Acrylic` backdrop with Acrylic default-on. Do not port custom interface/vendor colors, font customization UI, localized compact-token unit selection, Theme Code sharing, Settings/Refresh placement swapping, opacity/blur sliders, or the experimental Accent Blur implementation.

### Floating bubble

Retain the v1 compact quota-monitor semantics without porting its full tray-composer framework. v2 supports only `limitsAllSessions`, `icon`, `barsSession`, `barsWeekly`, `barsAllSessions`, and `bars`; token/cost text modes and `custom` composition remain excluded. `limitsAllSessions` is the v2 default and shows provider icons plus remaining quota percentages using the explicit v2 provider order. Preserve the v1 primary/secondary quota selection semantics, including canonical-window preference and ignoring additional Codex lanes in compact displays.

The v2 window shell deliberately changes the trigger semantics: Floating Bubble is default-on, the explicit minimize button collapses to the bubble, ordinary focus loss does not. If Bubble is disabled, minimize falls back to tray hide when the tray is enabled and otherwise to the OS minimize action. Close means quit. Variable-width bubble content keeps a fixed 34px logical height and must preserve docking, dragging, and destination-monitor DPI behavior.

### Dashboard history

Retain the user-facing activity heatmap and recent usage trend, but do not port the v1 persisted history/collector subsystem. v2 obtains long-range daily history directly from `tokscale graph`, normalizes only the daily usage/cost/activity fields needed by the renderer, and patches the current local-day bucket from live `getStats` data between history refreshes.

The compatibility facade retains `getDashboardHistory`; raw tokScale contribution-graph JSON must not cross that boundary.

### Session detail

Retain the usage/metadata semantics used by the existing Codex/Claude session drill-down. The downstream privacy rule is part of the behavior: raw prompt/response content and content-derived previews do not cross into renderer payloads. Provider-owned session titles are allowed metadata, but transcript-derived title fallbacks are not.

Primary v1 reference areas:

- `src/shared/sessionDetail.js`
- `src/shared/sessionDetailResolver.js`
- renderer code around `getSessionDetail`

Do not port unrelated session implementations for excluded providers.

### Codex quota parity and Business monthly credit

Retain the proven read-only provider quota semantics needed when tokScale has a Windows parity gap:

- healthy tokScale canonical quota remains authoritative;
- if tokScale has no usable base quota, reuse the existing Codex `auth.json` access token and v1 provider usage endpoint (`/wham/usage` on the default `backend-api`, otherwise `/api/codex/usage`) without refreshing or writing credentials;
- preserve the configured `chatgpt_base_url`, account/workspace id, FedRAMP request semantics, and account/workspace mismatch protection;
- Codex App Server may fill any canonical lane still missing and `account/rateLimits/read` supplements the Business/Team/Enterprise monthly `individualLimit`;
- preserve `limit`, `used`, `remainingPercent`, and `resetsAt` for the monthly credit window.

Relevant v1 history includes `a920202`, `9e6074d`, and their follow-up tests/merges.

### Claude quota parity

Retain only the provider-owned quota behavior missing from tokScale 4.15.1 on the tested Enterprise Windows path: reuse Claude Code access-token state, including provider-owned WSL `~/.claude/.credentials.json` discovery on Windows and Windows Credential Manager fallback, call `/api/oauth/usage`, fill missing 5-hour/weekly windows, and parse `spend` / `extra_usage` into the `Usage credits` monetary window. Preserve provider plan labels verbatim. For missing or rejected credentials, a bounded bare startup of the official Claude CLI may be scheduled in the background to delegate refresh back to Claude Code; Token Lens returns quota without waiting for the CLI, suppresses duplicate refresh launches, and consumes the refreshed credential on a later poll. Token Lens does not parse `/usage`, read/redeem refresh tokens, write credentials, or restore the general v1 account-management framework.

### Gemini CLI

Gemini CLI is now a first-class v2 provider even though the hardened v1 downstream allowlist excluded it. Retain upstream/tokScale semantics selectively:

- actual usage comes from tokScale's `gemini` client;
- quota follows Gemini CLI's Google Code Assist read path (`loadCodeAssist` then `retrieveUserQuota`) without copying OAuth refresh/onboarding/account-management behavior;
- credentials remain owned by Gemini CLI; current secure storage includes `gemini-cli-oauth/main-account` in the platform keychain and Gemini CLI's encrypted `gemini-credentials.json` file fallback, while the older `oauth_creds.json` source remains a compatibility input. For a missing, expired, or rejected credential, Token Lens may schedule one bounded bare startup of the official Gemini CLI in the background so Gemini itself refreshes/persists its credential; the foreground quota read returns immediately, duplicate launches are suppressed, and a later poll consumes the refreshed credential. Token Lens never reads/redeems refresh tokens or writes credentials itself;
- session identification may use provider-owned project metadata, but no Gemini transcript content is promoted into renderer metadata.

### Antigravity quota

Retain the minimum quota semantics from:

- local language-server process/port discovery and Connect quota RPCs;
- grouped Gemini and Claude/GPT 5-hour/weekly windows where exposed;
- conservative legacy model-family quota fallback;
- a remote Google Code Assist OAuth fallback seam only when an already-valid credential snapshot is explicitly supplied.

Use v1 `src/shared/antigravityProbe.js` and `src/shared/antigravityOAuth.js` as references, not as modules that must be copied intact. Do not port v1 managed-account login, OAuth refresh/write-back, onboarding, or multi-account storage. The v1 remote path owned its own Token Lens credentials; v2 must not reinterpret that as permission to consume another application's credential store.

## Do not port as modules or frameworks

Do not carry these v1 units into v2 as architectural owners:

- `src/electron/` runtime, preload, and Electron IPC plumbing;
- `src/shared/collector.js` as a collector framework;
- `src/shared/limitCollector.js` as a quota framework;
- Hub/server and multi-device synchronization;
- Worker deployment code;
- unrelated provider integrations;
- broad provider account-management flows;
- diagnostics/service-status subsystems;
- export infrastructure;
- macOS WidgetKit integration;
- old Electron packaging scripts and Electron-specific CI; preserve the v1 single-file no-install Windows UX and checksums through Tauri-native packaging instead of porting the old builder implementation. The v2 portable artifact embeds the pinned tokScale sidecar as compressed overlay data in the Tauri app PE and extracts only that sidecar to a guarded temp run directory, while the NSIS build keeps the normal adjacent `externalBin`; keep all release executables on the Windows GUI subsystem so no console window appears;
- upstream Token Monitor documentation that does not describe Token Lens v2.

When useful behavior exists inside one of these units, extract the smallest required semantic into the new architecture instead of importing the owner wholesale.

## Compatibility-facade rule

Keep the renderer-facing behavior required by v2, not the complete v1 preload surface.

Retain settings, usage/dashboard, session detail, floating/window, tray/view/appearance, dashboard controls, current app-update product behavior, tokScale lifecycle/status, basic utility calls, and read-only provider identity used by the retained UI.

Explicitly remove Hub, unrelated providers, account mutation, diagnostic repair, export, manual subscription-cost tracking, service status/reset forecast, and standalone model-pricing lookup.

## Porting acceptance test

Before accepting a v1-derived change, verify all of the following:

1. the behavior is required by the v2 architecture contract;
2. tokScale does not already provide the same responsibility;
3. the change does not pull an excluded provider/framework across the boundary;
4. renderer behavior remains compatible unless the task explicitly changes UX;
5. provider-specific logic is isolated behind the normalized v2 domain contract;
6. session-related changes preserve the v2 privacy invariant and do not surface prompt/response content or content-derived title fallbacks.

If these conditions are not met, leave the v1 code in the reference branch.
