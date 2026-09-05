# Token Lens v2 — Current State

## Current phase

Core tokScale data path, preserved renderer surface, fixed-range refresh semantics, floating-bubble behavior, and the retained native tray shell are implemented on `feat/v2-tokscale-vertical-slice`.

## Accepted baseline

- v2 is an orphan lineage with a clean codebase; v1 `main` is reference-only.
- desktop runtime: Tauri 2.
- supported tools: Codex, Claude, Antigravity only.
- tokScale is the primary actual-usage and supported-quota engine.
- Token Lens owns only confirmed tokScale gaps: Codex Business `individualLimit` and AGY quota.
- established Token Lens UI/UX is preserved rather than redesigned.
- model and session usage remain core product features; rich Codex/Claude session detail remains in scope.
- raw tokScale/provider schemas stay behind a stable normalization boundary.

## Current implementation

- tokScale 4.15.1 remains the pinned v2 baseline;
- stable Rust domain types own normalized usage, quota, tokScale status, credit, and reset-credit payloads;
- the Rust tokScale adapter collects today/week/month/all-time usage with model or session grouping;
- quota normalization admits only Codex, Claude, and Antigravity and keeps canonical/additional lanes distinct;
- Codex reset credits, ordinary credit status, and scalar spend-control state are retained from tokScale;
- structured Business `individualLimit` is deliberately not guessed from tokScale output;
- Tauri commands expose normalized reports; raw tokScale JSON is not a renderer contract;
- `window.tokenMonitor.getStats` composes the retained local stats contract and preserves serial tokScale scans;
- the compatibility payload retains client/model/session totals, token components, message counts, provider attribution, and cost;
- Codex reasoning follows the v1 additive public-total convention without double-counting Claude/AGY reasoning;
- the renderer now uses the original Token Lens stylesheet, icon assets, class vocabulary, 340x650 frameless transparent window geometry, and always-on-top behavior rather than the temporary skeleton UI;
- Home currently restores Limits and Models modules; Tools, Models, Sessions, and Limits detail views are wired to live `getStats` data;
- DAY / MONTH / TOTAL switching, manual refresh, view switching, close/minimize, drag region, and floating/normal pin toggle are wired through Tauri;
- the MONTH slot now preserves v1 `MONTH` / `WEEK` / `7D` / `30D` selection semantics, including locale-aware first-day-of-week behavior;
- derived Week / Last 7 / Last 30 ranges use a narrow `--since YYYY-MM-DD` tokScale command rather than reintroducing the v1 history subsystem;
- stats polling is visibility-aware at 30 seconds, while the compatibility layer caches Today (30s), Month (2m), derived ranges (1m), All Time (5m), and quota (5m); manual refresh bypasses all caches;
- overlapping renderer refreshes are serialized and coalesced so period changes cannot race an in-flight tokScale scan;
- the renderer controller is a small v2-specific implementation rather than a port of the v1 788 KB `app.js`;
- macOS transparent-window support is enabled through Tauri's `macos-private-api`; this implies macOS App Store distribution is not a target for this configuration;
- a minimal persisted settings store now owns floating-bubble enablement and click/hover trigger mode; bubble content is intentionally limited to the original icon-only mode for this slice;
- floating-bubble collapse/expand, left/right edge docking, drag-to-cursor movement, skip-taskbar behavior, always-on-top restoration, and original collapsed renderer classes are implemented through native Tauri window APIs;
- floating-bubble geometry is DPI-aware and recalculates the physical 34px-logical handle size when moving between monitors;
- Windows-specific bubble policy is explicit: collapse against full monitor bounds with zero edge margin, while macOS/other desktop targets use the work area with the original vertical margin;
- `x86_64-pc-windows-msvc` cargo check passes from macOS with the Windows target/toolchain resources, but this is compile validation rather than native Windows UX validation;
- local macOS native runtime smoke verifies the frameless main window plus actual collapse, native move, and expansion transitions; temporary smoke settings/source instrumentation were removed afterward;
- frontend production build, JS/Rust unit tests, live tokScale smoke including custom ranges, Clippy, rustfmt, native Tauri debug build, Windows target check, and npm audit pass;
- v2 GitHub Actions now runs the non-credentialed frontend/Rust checks, Clippy, npm audit, and native Tauri debug build on both `macos-latest` and `windows-latest`; the first native matrix run passed on both hosts;
- the retained native tray shell is restored with default-on visibility, the original macOS template icon, Today-token menu-bar title where supported, usage/cost tooltip, left-click focus/restore, Refresh Now, retained-view navigation, Settings, version, and Quit actions;
- with the tray enabled, window close now hides Token Lens instead of destroying the app, matching the v1 recoverability contract; disabling the tray restores normal close behavior;
- the renderer listens for narrow tray actions rather than reintroducing Electron IPC, and publishes only the small Today usage/cost summary needed by the native tray;
- the v2 settings surface now controls tray visibility as well as floating-bubble behavior;
- rich session detail, Codex Business enrichment, and AGY quota adapter are not implemented yet.

## Next action

Complete the preserved desktop-shell behavior without reintroducing v1 backend breadth:

1. validate the restored tray shell through native macOS/Windows builds and keep the tray surface limited to v2-retained actions;
2. restore rich Codex/Claude session detail against the normalized usage/session contract;
3. perform native Windows floating-bubble/tray UX validation before declaring cross-platform visual/interaction parity complete;
4. defer advanced v1 tray-composer/generated-bar modes unless they prove necessary to preserve the core Codex/Claude/AGY monitoring UX.

After the base desktop shell is stable, add Codex Business `individualLimit` and AGY quota as narrow adapters.

## Known open items

No architecture decision currently blocks implementation.

Packaging remains separate: development resolves the npm-installed native tokScale binary, but the production Tauri resource/sidecar path is not wired yet. Preserve the current Token Lens tokScale lifecycle policy when packaging is implemented.

Implementation-time validation still required:

- Claude quota against a fresh valid Claude credential;
- Codex Business `individualLimit` on a Business account after its thin adapter is implemented;
- AGY quota after the narrow adapter is extracted;
- packaged tokScale resolution on release artifacts;
- native Windows floating-bubble validation at common DPI scales (100/125/150%), multi-monitor movement, taskbar/skip-taskbar behavior, transparent mini-window chrome, and expansion restore;
- final macOS/Windows visual parity after tray behavior is restored.

## Authoritative references

- [`docs/architecture/v2-architecture.md`](docs/architecture/v2-architecture.md)
- [`docs/migration/v1-porting-map.md`](docs/migration/v1-porting-map.md)
