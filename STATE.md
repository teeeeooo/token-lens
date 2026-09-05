# Token Lens v2 — Current State

## Current phase

Core tokScale data path and first preserved renderer surface implemented on `feat/v2-tokscale-vertical-slice`.

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
- the renderer controller is a small v2-specific implementation rather than a port of the v1 788 KB `app.js`;
- macOS transparent-window support is enabled through Tauri's `macos-private-api`; this implies macOS App Store distribution is not a target for this configuration;
- local macOS native runtime smoke creates the frameless window cleanly after the transparency configuration is enabled;
- frontend production build, JS/Rust unit tests, live tokScale smoke, Clippy, rustfmt, native Tauri debug build, and npm audit pass;
- history/derived ranges, periodic refresh/cache, floating-bubble behavior, tray, settings, rich session detail, Codex Business enrichment, and AGY quota adapter are not implemented yet.

## Next action

Complete the preserved main-window behavior without reintroducing v1 backend breadth:

1. restore automatic refresh/cache semantics and derived Week / Last 7 / Last 30 usage ranges;
2. validate live model/session rendering and period switching;
3. implement floating-bubble and tray behavior from the original UX on the Tauri shell;
4. add only the minimal settings needed by those retained surfaces.

After the base desktop shell is stable, add Codex Business `individualLimit`, AGY quota, and rich Codex/Claude session detail as narrow adapters.

## Known open items

No architecture decision currently blocks implementation.

Packaging remains separate: development resolves the npm-installed native tokScale binary, but the production Tauri resource/sidecar path is not wired yet. Preserve the current Token Lens tokScale lifecycle policy when packaging is implemented.

Implementation-time validation still required:

- Claude quota against a fresh valid Claude credential;
- Codex Business `individualLimit` on a Business account after its thin adapter is implemented;
- AGY quota after the narrow adapter is extracted;
- packaged tokScale resolution on release artifacts;
- final macOS/Windows visual parity after tray/floating behavior is restored.

## Authoritative references

- [`docs/architecture/v2-architecture.md`](docs/architecture/v2-architecture.md)
- [`docs/migration/v1-porting-map.md`](docs/migration/v1-porting-map.md)
