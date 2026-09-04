# Token Lens v2 — Current State

## Current phase

v2 bootstrap / implementation-ready skeleton.

## Accepted baseline

- v2 is an orphan branch with a clean codebase; v1 `main` is reference-only.
- desktop runtime: Tauri 2.
- supported tools: Codex, Claude, Antigravity only.
- tokScale is the primary actual-usage and supported-quota engine.
- Token Lens owns only confirmed tokScale gaps: Codex Business `individualLimit` and AGY quota.
- established Token Lens UI/UX is preserved rather than redesigned.
- model and session usage remain core product features; rich Codex/Claude session detail remains in scope.
- raw provider/tokScale schemas stay behind a stable normalization boundary.

## Current implementation

- clean Tauri 2 + Vite skeleton exists;
- original Token Lens application icon is reused for Tauri desktop assets;
- tokScale 4.15.1 is pinned as the initial v2 baseline;
- frontend production build passes;
- Rust `cargo check` passes;
- Rust debug native build (`cargo build`) passes on macOS;
- no v1 renderer or provider logic has been ported yet.

## Next action

Implement the first vertical slice without widening scope:

1. define the normalized v2 usage/quota domain types;
2. add the tokScale adapter and prove actual-usage/quota ingestion;
3. add the minimal `window.tokenMonitor` compatibility bridge needed by the first preserved renderer surface;
4. port the existing renderer shell incrementally against that bridge.

Codex Business and AGY quota adapters should be added only after the base tokScale path is working.

## Known open items

No architecture decision currently blocks implementation.

Implementation-time validation still required:

- Claude quota against a fresh valid Claude credential;
- Codex Business `individualLimit` behavior on a Business account after its thin adapter is implemented;
- AGY quota behavior after the narrow adapter is extracted.

## Authoritative references

- [`docs/architecture/v2-architecture.md`](docs/architecture/v2-architecture.md)
- [`docs/migration/v1-porting-map.md`](docs/migration/v1-porting-map.md)
