# v1 → v2 Porting Map

## Purpose

This file controls reuse from the Electron-based Token Lens v1 codebase. It prevents v2 work from treating the entire v1 tree as migration scope.

Reference lineage at v2 bootstrap:

- v1 branch: `main`
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

Port renderer assets selectively; remove code paths for features excluded from v2 rather than recreating their backend APIs.

### Session detail

Retain the semantics used by the existing Codex/Claude session drill-down.

Primary v1 reference areas:

- `src/shared/sessionDetail.js`
- `src/shared/sessionDetailResolver.js`
- renderer code around `getSessionDetail`

Do not port unrelated session implementations for excluded providers.

### Codex Business monthly credit

Retain only the proven Business `individualLimit` enrichment semantics:

- base quota remains authoritative from tokScale/provider OAuth data;
- Codex App Server `account/rateLimits/read` supplements only the missing Business monthly credit window;
- preserve account/workspace mismatch protection;
- preserve `limit`, `used`, `remainingPercent`, and `resetsAt`.

Relevant v1 history includes `a920202`, `9e6074d`, and their follow-up tests/merges.

### Antigravity quota

Retain the minimum quota semantics from:

- local language-server probing;
- remote Google Cloud Code OAuth fallback.

Use v1 `src/shared/antigravityProbe.js` and `src/shared/antigravityOAuth.js` as references, not as modules that must be copied intact.

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
- old Electron packaging scripts and Electron-specific CI;
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
5. provider-specific logic is isolated behind the normalized v2 domain contract.

If these conditions are not met, leave the v1 code in the reference branch.
