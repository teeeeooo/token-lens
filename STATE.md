# Token Lens v2 — Current State

## Current phase

First vertical slice implemented and validated on `feat/v2-tokscale-vertical-slice`.

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

- clean Tauri 2 + Vite skeleton is buildable;
- tokScale 4.15.1 remains the pinned v2 baseline;
- stable Rust domain types now own normalized usage, quota, tokScale status, credit, and reset-credit payloads;
- the Rust tokScale adapter collects today/week/month usage with model or session grouping;
- quota normalization admits only Codex, Claude, and Antigravity and classifies non-canonical lanes as additional;
- Codex reset credits, ordinary credit status, and scalar spend-control state are retained from tokScale;
- structured Business `individualLimit` is deliberately not guessed from tokScale output;
- Tauri commands expose normalized usage/quota reports and tokScale status; raw tokScale JSON is not a renderer contract;
- the first `window.tokenMonitor` compatibility method, `getTokscaleStatus`, is installed through a frozen facade;
- local macOS live validation successfully normalized both current tokScale usage and quota;
- frontend production build, Rust unit tests, live tokScale smoke, Clippy, rustfmt, native debug build, and npm audit pass;
- the production v1 renderer, session-detail parser, Codex Business adapter, and AGY quota adapter are not ported yet.

## Next action

Build the first preserved renderer-facing path without widening scope:

1. map the minimum v1 `getStats` payload actually consumed by the retained dashboard/model/session surfaces;
2. compose normalized tokScale reports into that compatibility payload instead of exposing raw backend schemas;
3. port the corresponding v1 renderer shell/assets against the compatibility facade;
4. add refresh/cache behavior only as required by the preserved renderer contract.

After the base renderer path works, add Codex Business `individualLimit`, AGY quota, and rich Codex/Claude session detail as narrow adapters.

## Known open items

No architecture decision currently blocks the next implementation slice.

Packaging work remains intentionally separate: the adapter can resolve the npm-installed native tokScale binary during development, but the Tauri production resource/sidecar packaging path has not yet been wired. Preserve the current Token Lens tokScale lifecycle policy when that work is implemented.

Implementation-time validation still required:

- Claude quota against a fresh valid Claude credential;
- Codex Business `individualLimit` on a Business account after its thin adapter is implemented;
- AGY quota after the narrow adapter is extracted;
- packaged tokScale resolution on release artifacts.

## Authoritative references

- [`docs/architecture/v2-architecture.md`](docs/architecture/v2-architecture.md)
- [`docs/migration/v1-porting-map.md`](docs/migration/v1-porting-map.md)
