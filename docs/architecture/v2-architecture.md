# Token Lens v2 Architecture Contract

## Status

Accepted baseline for v2 implementation.

This document owns durable v2 architecture decisions. Current progress belongs in [`../../STATE.md`](../../STATE.md).

## Optimization criterion

The v2 design is not judged by how much of Token Lens v1 can be migrated. It is judged by how small and maintainable the current Codex/Claude/Gemini CLI/Antigravity quota-and-usage monitor can be while preserving the established Token Lens user experience and a narrow path for future provider adapters.

## Product scope

The current supported provider set is:

- Codex
- Claude
- Gemini CLI
- Antigravity / AGY

This is an explicit runtime allowlist, not a permanent architectural ceiling. A later provider may be added through a deliberate provider adapter plus usage/quota authority, credential-boundary, privacy-contract, and regression-test review. Adding a provider must not require redesigning the normalized domain or renderer.

Required user-facing capabilities:

- current account quota and reset information;
- short-window/session quota and weekly quota where the provider exposes them;
- actual usage by day/week/month;
- model-level usage;
- session-level usage;
- token-category breakdown when available;
- cost when tokScale reports a reliable value;
- existing main dashboard, limits presentation, tray, and floating monitor behavior.

## Explicit non-goals

Unless required by a dependency of the supported v2 product, do not carry forward:

- Electron runtime, preload, or Electron IPC architecture;
- Hub / multi-device sync;
- Discord RPC;
- broad provider/account-management frameworks;
- unrelated v1 provider integrations such as OpenRouter, Mimo, Qoder, Trae, Z.ai, Ollama, Cursor, OpenCode, Copilot, and similar integrations unless a later explicit provider-adapter decision adds one;
- general service-status or diagnostic subsystems;
- export/server features;
- macOS WidgetKit integration;
- v1 collector or limit-collector modules as architectural units.

## Runtime boundary

v2 uses Tauri 2 as the desktop shell. The frontend remains web technology rendered by the platform webview; Electron and bundled Chromium are removed.

The old Node/Electron backend is a reference source only. Do not translate it wholesale into Rust.

The intended runtime flow is:

```text
existing renderer / preserved UX
        ↓
window.tokenMonitor compatibility facade
        ↓
stable Token Lens v2 domain contracts
        ↓
Tauri commands/events
        ↓
tokScale adapter + narrow provider adapters
```

## UI/UX preservation rule

v2 is not a redesign project.

Preserve the established Token Lens renderer behavior and visual language wherever practical, including:

- dashboard layout and quota presentation;
- model/session exploration;
- typography, icons, spacing, theme, and interaction conventions;
- floating-bubble behavior;
- tray behavior;
- dashboard window behavior and common controls.

Port UI assets and behavior intentionally from v1. Runtime-specific Electron code must be replaced at the compatibility boundary rather than allowed to reshape the product.

### Appearance subset

Appearance parity is intentionally narrower than v1. Preserve only the controls that materially affect readability, monitor compatibility, or the established visual identity:

- keep the built-in theme presets only; do not expose per-color interface overrides or vendor-color overrides;
- keep zoom as a persisted user control for mixed-DPI and monitor-scale compatibility;
- follow the operating system `prefers-reduced-motion` preference directly; do not expose a separate Reduce Motion setting;
- keep the live indicator and provider/tool icons as always-on product UI rather than user-configurable toggles;
- keep compact total-token display as a user option, using the fixed international `K/M/B` unit convention; do not expose alternate localized unit systems;
- on Windows, provide only `Off` / `Acrylic` backdrop selection, defaulting to Acrylic; do not port v1 opacity/blur sliders or the experimental Accent Blur path;
- Acrylic must be disabled while the floating bubble is collapsed and restored on expansion when enabled; unsupported Windows environments fall back to the normal transparent surface without failing the app;
- remove custom font selection, Theme Code import/export, Settings/Refresh placement swapping, and other fine-grained appearance composition controls.

The Windows Acrylic implementation must use a supported Tauri/Windows integration path. Do not reintroduce v1's undocumented `SetWindowCompositionAttribute` Accent Blur implementation merely for visual parity.

## Primary data engine

tokScale is the primary data engine for v2.

Use tokScale for:

- Codex actual usage;
- Claude actual usage;
- Gemini CLI actual usage;
- Antigravity actual usage;
- daily history/contribution data used by retained dashboard activity and trend charts;
- model aggregation;
- session aggregation where tokScale provides it;
- input/output/cache/reasoning token breakdown;
- cost calculation;
- Codex base subscription quota;
- Claude base subscription quota.

Do not duplicate tokScale parsing or provider logic merely to preserve v1 internals.

## Provider authority and enrichment

### Codex

- tokScale is authoritative for normal quota windows and actual usage.
- tokScale also supplies reset-credit information and ordinary credit/spend-control status when available.
- Codex App Server is a supplement only for Business/Team/Enterprise monthly `individualLimit` data that tokScale 4.15.1 does not expose as a structured quota window.
- App Server enrichment must never replace healthy tokScale/OAuth base quota data.
- Workspace/account mismatch protection from v1 remains a required semantic when enriching Business quota.

The Business monthly credit window preserves:

- absolute credit limit;
- used credits;
- remaining percentage;
- reset timestamp.

### Claude

- tokScale is authoritative for Claude quota and actual usage in the initial v2 scope.
- 5-hour, weekly, and scoped/Opus quota exposed by tokScale should be normalized and shown.
- v1 prepaid-credit and monthly monetary enrichment are not part of the initial v2 scope.
- do not create a separate Claude account-management framework.

### Gemini CLI

- tokScale is authoritative for Gemini CLI actual usage, including model/session aggregation and token/cost data.
- Gemini CLI quota is a narrow read-only Google Code Assist enrichment because tokScale 4.15.1 does not expose Gemini quota through `tokscale usage --json`.
- reuse the Gemini CLI-owned `~/.gemini/oauth_creds.json` access token only while it is already valid; Token Lens must not refresh OAuth, write credentials, call `onboardUser`, or create/attach a Google Cloud project.
- follow Gemini CLI's `loadCodeAssist` → `retrieveUserQuota` flow and keep model identifiers allowlist-free behind the normalized quota contract.
- Gemini session identification may use the provider-owned `tmp/<project-key>` location plus `projects.json` path-to-key mapping to expose only a project basename; prompt/response-derived title fallbacks remain prohibited.

### Antigravity / AGY

- tokScale is authoritative for actual usage.
- quota remains a narrow Token Lens adapter because tokScale subscription quota does not currently cover AGY.
- local quota authority is the Antigravity language-server Connect surface, preferring detected app, `agy`/CLI, then IDE processes and `RetrieveUserQuotaSummary` before legacy model-config fallbacks.
- grouped local quota preserves Gemini and Claude/GPT families with 5-hour and weekly cadence when the language server exposes those buckets.
- local HTTPS certificate bypass is permitted only for the hard-coded `127.0.0.1` language-server transport; external Google HTTPS keeps normal certificate verification.
- remote Google Code Assist OAuth remains a fallback seam only for an already-valid externally supplied credential snapshot. Token Lens must not create an Antigravity login flow, refresh OAuth, persist credentials, or silently consume credentials owned by another monitor.
- until an Antigravity-owned credential source is independently confirmed, absence of a local language server and an explicit valid OAuth snapshot means AGY quota is unavailable rather than guessed from another Google login.
- port only the quota semantics and minimum read-only credential behavior required by this path, not the v1 general account-management framework.

## Security and privacy boundary

Token Lens is a local usage/quota monitor, not a transcript viewer or authentication manager. Local provider session files may contain sensitive developer content and may be parsed locally only when required to derive approved usage metadata.

Renderer-facing session data may include:

- provider/client and session identifier;
- provider-owned display title metadata;
- project/workspace label or a path basename;
- timestamps and turn/exchange structure;
- model, token/cache/reasoning counts, message counts, tool names/types, and cost attribution.

Provider-owned titles are permitted metadata when the provider already persists them as a distinct session/thread title, for example Codex `display_title` / `title` or Claude `aiTitle`. Token Lens must not synthesize a title from transcript content. The title fallback order is provider-owned title, then project/workspace label or path basename, then session identifier.

The renderer contract must not contain raw prompt/response content or content-derived previews. In particular, do not expose or use as title fallbacks:

- prompt text or prompt previews;
- `first_user_message`, `lastPrompt`, or equivalent first/last user-message fields;
- assistant response text or response previews;
- reasoning summaries or arbitrary transcript snippets;
- a title generated by sending transcript content to another model.

Full working-directory paths should remain behind the backend boundary unless a separate explicit UX/privacy decision requires them; a basename/project label is sufficient for session identification. These restrictions are durable security invariants and should be regression-tested at the normalized payload boundary.

Authentication state remains owned by the original coding tools. Provider adapters should read only the minimum existing local state required by their monitor responsibilities and must not create a general Token Lens credential store or account-management framework.

The renderer must run under an explicit Content Security Policy rather than an unrestricted WebView policy. Script execution stays limited to packaged application code (`script-src 'self'`) with no inline-script or eval relaxation. Tauri IPC is the only non-self renderer connection surface. The retained renderer currently requires dynamic inline styles, so `style-src 'self' 'unsafe-inline'` is an accepted narrow compatibility exception; tightening that exception may happen later without changing the product contract.

## Normalization boundary

Raw tokScale JSON and raw provider/RPC responses must not become renderer contracts.

The backend owns a small stable domain model with four response families:

1. `UsageReport` — period totals plus model/session rows and token/cost fields.
2. `QuotaReport` — provider/account identity plus normalized quota windows and provider-specific credit fields.
3. `HistoryReport` — normalized daily usage/cost/activity rows plus long-range summary fields sourced from `tokscale graph`; raw graph JSON is not a renderer contract.
4. `SessionDetail` — Codex/Claude per-turn usage and session metadata retained from the proven v1 behavior, subject to the privacy boundary above.

A normalized usage row may contain:

```text
client, provider, model, sessionId?
input, output, cacheRead, cacheWrite, reasoning
messageCount, cost
```

A normalized quota window may contain:

```text
kind, label, metric
usedPercent, remainingPercent
used?, limit?, remaining?
currency?
resetsAt?
source
```

Provider-specific fields should be preserved only when the UI has a defined use for them. Unknown raw fields stay behind the adapter boundary.

The `window.tokenMonitor` compatibility layer may shape these normalized results into the payload expected by the preserved renderer, but the renderer must never depend directly on tokScale schema details.

## Renderer compatibility facade

Preserve only the API surface that the retained v2 renderer actually calls. The current `window.tokenMonitor` facade is intentionally small:

- usage/dashboard: `getStats`, `getDashboardHistory`, `getSessionDetail`, `getTokscaleStatus`;
- settings: `getSettings`, `updateSettings`;
- floating monitor: `getFloatingBubbleState`, `collapseFloatingBubbleIfIdle`, `expandFloatingBubble`, `peekFloatingBubble`, `moveFloatingBubble`;
- tray summary: `updateTraySummary`.

Native window controls such as minimize, close, drag, always-on-top, and window state use Tauri APIs directly. Tray navigation arrives through a narrow Tauri event rather than a recreated Electron IPC surface.

Do not recreate v1 facade APIs for unreachable or explicitly removed UI. Hub/multi-device, unrelated provider facades, credential/account mutation, account switching, diagnostics/repair, export/session archive management, service status, updater/download surfaces, manual subscription-cost tracking, and standalone model-pricing lookup remain out of scope unless a later architecture decision makes one reachable again.

Removing standalone model-pricing lookup does not remove model/session usage. tokScale supplies model/session aggregation and row cost directly.

## Session-detail contract

Model/session aggregation is a core v2 feature, not legacy baggage.

- tokScale provides the model and session aggregate rows used by the dashboard.
- preserve the existing Codex/Claude session usage/metadata drill-down, not a transcript-content viewer.
- provider-owned session titles may be shown as approved metadata; prompt/response-derived title fallbacks are prohibited.
- the retained detail parser should be extracted as a small dependency, not brought over as part of the old collector architecture.
- AGY per-turn detail is not required in the initial provider contract unless a later explicit product decision adds it.

## tokScale lifecycle

Preserve the current Token Lens tokScale distribution/update policy rather than designing a new lifecycle as part of v2.

The v2 implementation pins tokScale per Token Lens release and stages the matching platform-native npm package as a Tauri external binary. Production discovery prefers an embedded portable payload when the current executable carries one, otherwise the adjacent bundled sidecar; development may fall back to the pinned project package, and `TOKEN_LENS_TOKSCALE_BIN` remains an explicit developer/test override.

The hardened Token Lens policy remains **no runtime tokScale download/update**. npm package identity and the staged binary's `--version` are validated at build time, and packaged runtime status exposes the bundled source/version. Windows distribution preserves the v1 single-file portable UX without changing the runtime sidecar boundary: the no-install `Token-Lens-<version>.exe` is the normal Tauri app PE with the pinned `tokscale.exe` appended as a compressed payload. When the portable payload is present, runtime discovery extracts only tokScale into an isolated system-temp run directory, holds an active-run lock, removes that directory on normal process exit, and removes unlocked stale run directories on a later portable launch. NSIS installations continue to use the normal adjacent Tauri `externalBin` sidecar. Windows release executables must use the GUI PE subsystem so launching the desktop app never opens a console window; the packaging gate verifies unsigned GUI subsystem output for the NSIS installer, the normal app executable, and the single-file portable artifact.

The exact implementation may change to fit Tauri, but the user-facing lifecycle policy does not change merely because the shell changed.

## Upstream and maintenance relationship

Token Lens v2 is an independent downstream application derived from Token Monitor rather than a source-tree continuation of it.

- v1 `main` remains a reference implementation and historical evidence source.
- original Token Monitor changes are reviewed for relevant semantics, not routinely merged into v2.
- tokScale is the primary evolving dependency for usage and supported subscription-quota behavior.
- direct Token Lens provider adapters should stay narrow enough that provider changes have a small, explicit maintenance surface.

## Implementation discipline

When a v1 behavior is needed, identify the smallest semantic or UI unit that owns it and port that unit deliberately. Do not pull transitive v1 infrastructure into v2 unless the architecture contract explicitly requires it.
