# Token Lens v2 — Current State

## Current phase

Core tokScale data path, preserved renderer surface, fixed-range refresh semantics, floating-bubble behavior, and the retained native tray shell are implemented on `feat/v2-tokscale-vertical-slice`.

## Accepted baseline

- v2 is an orphan lineage with a clean codebase; v1 `main` is reference-only.
- desktop runtime: Tauri 2.
- current supported provider set: Codex, Claude, Gemini CLI, Antigravity; future providers require an explicit adapter/security/data-contract review rather than a core redesign.
- tokScale is the primary actual-usage and supported-quota engine.
- Token Lens owns only confirmed tokScale gaps: Codex Business `individualLimit`, Gemini CLI quota, and AGY quota.
- established Token Lens UI/UX is preserved rather than redesigned.
- model and session usage remain core product features; Codex/Claude session usage/metadata detail remains in scope, while raw prompt/response content remains outside the renderer contract.
- raw tokScale/provider schemas stay behind a stable normalization boundary; provider-owned session titles are approved metadata, but transcript-derived title fallbacks are prohibited.

## Current implementation

- tokScale 4.15.1 remains the pinned v2 baseline;
- stable Rust domain types own normalized usage, quota, tokScale status, credit, and reset-credit payloads;
- the Rust tokScale adapter collects today/week/month/all-time usage with model or session grouping;
- quota normalization admits only Codex, Claude, Gemini CLI, and Antigravity and keeps canonical/additional lanes distinct;
- Codex reset credits, ordinary credit status, and scalar spend-control state are retained from tokScale;
- structured Business `individualLimit` is deliberately not guessed from tokScale output; a narrow Codex App Server enrichment supplies it only when tokScale does not already expose an absolute credits window;
- Tauri commands expose normalized reports; raw tokScale JSON is not a renderer contract;
- `window.tokenMonitor.getStats` composes the retained local stats contract and preserves serial tokScale scans;
- the compatibility payload retains client/model/session totals, token components, message counts, provider attribution, and cost;
- Codex reasoning follows the v1 additive public-total convention without double-counting Claude/Gemini/AGY reasoning;
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
- macOS-hosted `x86_64-pc-windows-msvc` checks remain supplemental only: native C dependencies such as the bundled SQLite used for read-only Codex title metadata require a Windows CRT/SDK that is not present on the Mac host, so the authoritative Windows compile/build gate is the GitHub `windows-latest` native job;
- local macOS native runtime smoke verifies the frameless main window plus actual collapse, native move, and expansion transitions; temporary smoke settings/source instrumentation were removed afterward;
- frontend production build, JS/Rust unit tests, live tokScale/session-metadata/session-detail smoke, Clippy, rustfmt, native macOS Tauri debug build, and npm audit pass locally; Windows compile/build validation is owned by the native GitHub Actions job rather than a macOS cross-target check;
- v2 GitHub Actions now runs the non-credentialed frontend/Rust checks, Clippy, npm audit, and native Tauri debug build on both `macos-latest` and `windows-latest`; the first native matrix run passed on both hosts;
- the retained native tray shell is restored with default-on visibility, the original macOS template icon, Today-token menu-bar title where supported, usage/cost tooltip, left-click focus/restore, Refresh Now, retained-view navigation, Settings, version, and Quit actions;
- with the tray enabled, window close now hides Token Lens instead of destroying the app, matching the v1 recoverability contract; disabling the tray restores normal close behavior;
- the renderer listens for narrow tray actions rather than reintroducing Electron IPC, and publishes only the small Today usage/cost summary needed by the native tray;
- the v2 settings surface now controls tray visibility as well as floating-bubble behavior;
- session-list identification now uses a separate read-only provider-metadata adapter rather than transcript text: Codex resolves provider-owned `display_title` / `title` from local Codex SQLite state/catalog data, while Claude resolves explicit `aiTitle` metadata from its JSONL session files;
- renderer session-title fallback is provider-owned title → project/path basename → session identifier, with client/model/message information kept as secondary context; full working-directory paths and raw prompt/response fields are not part of the normalized metadata payload;
- session metadata is requested only while the Sessions view is active, then batch-enriched and cached independently from tokScale usage so metadata lookup failure cannot take down usage/quota reporting; payload-shape regression tests and a local ignored live smoke enforce the privacy boundary;
- Codex/Claude per-turn session usage/metadata detail is now restored through a small Rust parser invoked with `spawn_blocking`; it resolves only the selected local session file and emits timestamps, exchange/turn structure, token/cache/reasoning buckets, tool names, and proportional cost attribution;
- the detail parser deserializes only structural/usage/tool metadata, never prompt/response text or previews; Claude `isMeta` and `tool_result` records do not create user-exchange boundaries, while duplicate Claude message blocks are de-duplicated by message id/line uuid;
- Sessions rows for Codex/Claude now drill into the retained detail surface with neutral `Exchange #N` / `Reply #N` labels, newest/most-tokens sorting, expandable turn rows, and current-period filtering using an absolute local-calendar range start;
- Codex raw reasoning is normalized into the same disjoint additive bucket used by the v2 tokScale compatibility contract, while Claude retains its provider output semantics;
- Codex Business `individualLimit` enrichment is now implemented as a best-effort supplement to healthy tokScale quota data: Token Lens captures the selected local Codex workspace before the tokScale read, requires that workspace to remain unchanged across the RPC probe, compares account email when both sides expose one, and discards the enrichment on any mismatch or App Server failure;
- the App Server probe runs `codex -s read-only -a untrusted app-server`, calls only `account/read` with `refreshToken: false` plus `account/rateLimits/read`, and materializes a single canonical Monthly `CREDITS` window while leaving tokScale session/weekly/additional lanes authoritative;
- the retained Limits renderer now preserves capability-based absolute quota metadata and displays Business monthly remaining/total credits without Codex-specific formatting logic;
- local live smoke confirms the installed Codex App Server transport/schema path; an actual Business account is still required to validate live `individualLimit` values end to end;
- Gemini CLI actual usage is now admitted directly from tokScale as a first-class `gemini` client/provider; the renderer keeps Gemini CLI distinct from Antigravity while Google/Gemini model rows use the Gemini visual identity;
- Gemini session metadata uses only the local session header `sessionId`, its provider-owned `tmp/<project-key>` location, and `projects.json` path-to-key mapping, exposing only the project basename and never deriving a title from conversation content;
- Gemini quota uses a best-effort read-only `loadCodeAssist` → `retrieveUserQuota` adapter only when the existing Gemini OAuth access token is still valid; Token Lens neither refreshes nor persists Google credentials and never calls onboarding/project-creation APIs;
- Gemini model quota windows remain allowlist-free and Limits shows all returned model buckets, while Home selects the two lowest remaining percentages;
- the AGY quota adapter is not implemented yet.

## Next action

Complete the preserved desktop-shell behavior without reintroducing v1 backend breadth:

1. add the AGY quota adapter using the local language-server path plus the approved OAuth fallback;
2. perform native Windows floating-bubble/tray/session-detail UX validation before declaring cross-platform visual/interaction parity complete;
3. defer advanced v1 tray-composer/generated-bar modes unless they prove necessary to preserve the core monitoring UX.

## Known open items

No architecture decision currently blocks implementation.

Packaging remains separate: development resolves the npm-installed native tokScale binary, but the production Tauri resource/sidecar path is not wired yet. Preserve the current Token Lens tokScale lifecycle policy when packaging is implemented.

Implementation-time validation still required:

- Claude quota against a fresh valid Claude credential;
- Codex Business `individualLimit` end-to-end validation on an actual Business account;
- Gemini CLI quota end-to-end validation while the provider-owned OAuth access token is already valid;
- AGY quota after the narrow adapter is extracted;
- packaged tokScale resolution on release artifacts;
- native Windows floating-bubble validation at common DPI scales (100/125/150%), multi-monitor movement, taskbar/skip-taskbar behavior, transparent mini-window chrome, and expansion restore;
- final macOS/Windows visual parity after tray behavior is restored.

## Authoritative references

- [`docs/architecture/v2-architecture.md`](docs/architecture/v2-architecture.md)
- [`docs/migration/v1-porting-map.md`](docs/migration/v1-porting-map.md)
