# Token Lens v2 — Current State

## Current phase

First tokScale/renderer-compatibility vertical slice implemented and validated on `feat/v2-tokscale-vertical-slice`.

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
- stable Rust domain types own normalized usage, quota, tokScale status, credit, and reset-credit payloads;
- the Rust tokScale adapter collects today/week/month/all-time usage with model or session grouping;
- quota normalization admits only Codex, Claude, and Antigravity;
- canonical and additional quota lanes remain distinct while preserving the lane's temporal kind where it can be inferred;
- Codex reset credits, ordinary credit status, and scalar spend-control state are retained from tokScale;
- structured Business `individualLimit` is deliberately not guessed from tokScale output;
- Tauri commands expose normalized usage/quota reports and tokScale status; raw tokScale JSON is not a renderer contract;
- `window.tokenMonitor.getStats` now composes a v1-compatible local stats payload from normalized tokScale reports;
- `getStats` preserves the v1 serial scan discipline and exposes `today`, `month`, and `allTime` periods;
- the compatibility period keeps client/model/session totals, token components, message counts, provider attribution, and tokScale cost;
- Codex reasoning remains additive to the public total/output convention used by v1, while Claude/AGY reasoning is not double-counted;
- normalized quota is mapped into the retained `limits.providers[].windows[]` presentation shape, including reset credits;
- the frozen compatibility facade currently exposes `getStats` and `getTokscaleStatus` only;
- local macOS live validation successfully normalized current usage, all-time usage, and quota;
- frontend production build, JS/Rust unit tests, live tokScale smoke, Clippy, rustfmt, native debug build, and npm audit pass;
- the production v1 renderer, history/derived ranges, session-detail parser, Codex Business adapter, and AGY quota adapter are not ported yet.

## Next action

Port the first real v1 renderer surface against the compatibility facade without widening scope:

1. bring over the dashboard/model/session renderer shell and only its true dependency closure;
2. remove or stub UI paths for explicitly out-of-scope v1 features instead of porting their backends;
3. restore refresh/cache and derived week/last-7/last-30 behavior required by the preserved usage UI;
4. validate model and session views against live normalized tokScale data.

After the base renderer path works, add Codex Business `individualLimit`, AGY quota, and rich Codex/Claude session detail as narrow adapters.

## Known open items

No architecture decision currently blocks the next implementation slice.

Packaging work remains intentionally separate: the adapter resolves the npm-installed native tokScale binary during development, but the Tauri production resource/sidecar packaging path has not yet been wired. Preserve the current Token Lens tokScale lifecycle policy when that work is implemented.

Implementation-time validation still required:

- Claude quota against a fresh valid Claude credential;
- Codex Business `individualLimit` on a Business account after its thin adapter is implemented;
- AGY quota after the narrow adapter is extracted;
- packaged tokScale resolution on release artifacts.

## Authoritative references

- [`docs/architecture/v2-architecture.md`](docs/architecture/v2-architecture.md)
- [`docs/migration/v1-porting-map.md`](docs/migration/v1-porting-map.md)
