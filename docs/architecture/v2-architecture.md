# Token Lens v2 Architecture Contract

## Status

Accepted baseline for v2 implementation.

This document owns durable v2 architecture decisions. Current progress belongs in [`../../STATE.md`](../../STATE.md).

## Optimization criterion

The v2 design is not judged by how much of Token Lens v1 can be migrated. It is judged by how small and maintainable a Codex/Claude/Antigravity quota-and-usage monitor can be while preserving the established Token Lens user experience.

## Product scope

Token Lens v2 supports only:

- Codex
- Claude
- Antigravity / AGY

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
- unrelated providers such as OpenRouter, Mimo, Qoder, Trae, Z.ai, Ollama, Cursor, OpenCode, Copilot, and similar v1 integrations;
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

## Primary data engine

tokScale is the primary data engine for v2.

Use tokScale for:

- Codex actual usage;
- Claude actual usage;
- Antigravity actual usage;
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

### Antigravity / AGY

- tokScale is authoritative for actual usage.
- quota remains a narrow Token Lens adapter because tokScale subscription quota does not currently cover AGY.
- prefer the existing local language-server quota path when available.
- retain the remote Google Cloud Code OAuth path as fallback when required for quota visibility without the app running.
- port only the quota semantics and minimum credential behavior required by this path, not the v1 general account-management framework.

## Normalization boundary

Raw tokScale JSON and raw provider/RPC responses must not become renderer contracts.

The backend owns a small stable domain model with three response families:

1. `UsageReport` — period totals plus model/session rows and token/cost fields.
2. `QuotaReport` — provider/account identity plus normalized quota windows and provider-specific credit fields.
3. `SessionDetail` — rich Codex/Claude per-turn detail retained from the proven v1 behavior.

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

Preserve only the API surface required by the retained v2 UX.

Keep these capability groups:

- settings: `getSettings`, `updateSettings`, settings push;
- usage/dashboard: `getStats`, `getDashboardHistory`, stats/history push, `getSessionDetail`;
- floating/window: expand, move, peek, idle-collapse, collapsed-size, minimize, close, state push;
- tray/view/appearance: tray icons, appearance preview, view state, theme/open-view/window-visibility events;
- dashboard window controls;
- current Token Lens app-update state/policy at the product-behavior level;
- current Token Lens tokScale lifecycle/status behavior;
- basic utilities such as app info, external links, clipboard, and user-data location;
- read-only Codex/Antigravity account identity needed by retained UI.

Remove these capability groups from v2:

- Hub/multi-device APIs;
- unrelated provider facades;
- general credential/account mutation;
- client-source repair/diagnostic APIs;
- export and retained-session archive management;
- service status and Codex reset forecast;
- manual subscription-cost tracking;
- standalone model-pricing lookup.

Removing standalone model-pricing lookup does not remove model/session usage. tokScale supplies model/session aggregation and row cost directly.

## Session-detail contract

Model/session aggregation is a core v2 feature, not legacy baggage.

- tokScale provides the model and session aggregate rows used by the dashboard.
- preserve the existing rich Codex/Claude session-detail behavior for drilling into a selected session.
- the retained detail parser should be extracted as a small dependency, not brought over as part of the old collector architecture.
- AGY rich per-turn detail is not required unless a later explicit product decision adds it.

## tokScale lifecycle

Preserve the current Token Lens tokScale distribution/update policy rather than designing a new lifecycle as part of v2.

The v2 implementation should continue to pin and validate the tokScale version used by a Token Lens release, preserve the existing bundled/runtime selection semantics that are still relevant, and keep update/reset behavior compatible with the current Token Lens product policy.

The exact implementation may change to fit Tauri, but the user-facing lifecycle policy does not change merely because the shell changed.

## Upstream and maintenance relationship

Token Lens v2 is an independent downstream application derived from Token Monitor rather than a source-tree continuation of it.

- v1 `main` remains a reference implementation and historical evidence source.
- original Token Monitor changes are reviewed for relevant semantics, not routinely merged into v2.
- tokScale is the primary evolving dependency for usage and supported subscription-quota behavior.
- direct Token Lens provider adapters should stay narrow enough that provider changes have a small, explicit maintenance surface.

## Implementation discipline

When a v1 behavior is needed, identify the smallest semantic or UI unit that owns it and port that unit deliberately. Do not pull transitive v1 infrastructure into v2 unless the architecture contract explicitly requires it.
