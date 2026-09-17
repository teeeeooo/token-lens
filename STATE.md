# Token Lens v2 — Current State

## Current phase

Token Lens v2 is the production line on `main`; the Electron-based v1 implementation remains preserved on `v1-legacy` only as a reference. The current Windows baseline has been exercised with live Codex/Claude/Gemini quota, hidden real-console auth recovery, selectable floating-bubble providers, and the Model/Session filter/scroll refinements.

## Accepted baseline

- Desktop runtime: Tauri 2 + Rust.
- Current provider allowlist: Codex, Claude Code, Gemini CLI, Antigravity (AGY). The set is extensible only through an explicit adapter plus authority/privacy/data-contract review.
- tokScale 4.15.1 is the pinned primary engine for actual usage, supported quota, model/session aggregation, token categories, cost, and dashboard history.
- Token Lens-owned provider code is limited to confirmed gaps: Codex quota/Business `individualLimit`, Claude OAuth usage/spend enrichment, Gemini Code Assist quota, AGY quota, and retained session metadata/detail behavior.
- The product is a local monitor, not an account manager or transcript viewer. Token Lens does not redeem refresh tokens, write provider credentials, or expose prompt/response content to the renderer.
- Established Token Lens UI/UX remains the design baseline; v2 intentionally omits retired Hub/sync, broad account management, export, updater, service-status, and other Electron-only subsystems.

## Current implementation

### Usage, quota, and refresh

- Rust owns stable normalized usage/history/quota/session contracts; raw tokScale/provider payloads do not cross into the renderer contract.
- First launch is progressive: Today renders before slower quota/Month/All-Time work completes. Non-forced duplicate loads join the newest in-flight read, including a manual refresh; only the current request may populate cache. Superseded provider-recovery results cannot overwrite a newer full quota snapshot.
- Missing quota percentages remain unknown rather than becoming 0%; a supplied used percentage may fill a missing remaining percentage. Genuine zero remains zero.
- Full quota reads tokScale once, then runs Codex/Claude/Gemini/AGY enrichment concurrently and merges only each adapter's provider row.
- Claude/Gemini credential recovery is separate from ordinary quota polling. While a credential is rejected, Token Lens does not re-probe the provider API with the same credential; provider-owned credential state must change before direct probing resumes.
- Provider `Retry-After` deadlines survive restart in `provider-rate-limits.json`; Claude/Gemini also use the bounded in-process 5/15/30/60-minute repeated-429 protection.
- Provider status/recovery/timestamps are explicit fields; diagnostic wording does not drive polling. Last-good snapshots retain their original capture time and are discarded on observed credential changes.
- Recovery diagnostics use `CREDENTIAL_RECOVERY` / `credential_recovery`; the originating failure is retained separately as `triggerCode` / `triggerStage`.

### Provider-specific status

- **Codex:** healthy tokScale quota wins. Read-only OAuth usage may fill missing canonical quota, then App Server may fill remaining lanes and Business/Team/Enterprise `individualLimit`; account/workspace mismatch guards remain mandatory.
- **Claude:** read-only `/api/oauth/usage` may fill missing 5-hour/weekly quota and monetary `Usage credits`. On Windows, rejected/missing credentials delegate refresh to a hidden real console running bare `claude`; a Job Object owns the process tree. Live Windows validation confirmed provider-owned credential rotation and quota recovery without user input. Non-Windows keeps the PTY + `/status` touch.
- **Gemini CLI:** actual usage comes from tokScale; quota uses read-only `loadCodeAssist` → `retrieveUserQuota`. Windows auth-touch uses hidden real-console `gemini --list-sessions -e none --skip-trust` from an isolated temp working directory, with no model prompt and no output scraping. Expired-token live validation and repeated unattended refreshes are confirmed.
- **Antigravity:** local quota probes only the detected loopback language-server surface, preferring grouped quota. Remote OAuth remains an explicit already-valid snapshot seam only; Token Lens does not create or refresh AGY credentials.

### Renderer and desktop shell

- The renderer keeps the Token Lens visual language while using a small v2-specific controller rather than the v1 Electron application controller.
- First-launch expanded size is 380×720 logical px; saved normal bounds are restored without allowing floating-bubble geometry to overwrite them.
- Appearance is intentionally narrow: Default/Obsidian/Porcelain, 70–160% zoom, optional compact totals, system UI font, OS reduced-motion preference, `Auto (system)` plus EN/KO/JA/ZH-CN/ZH-TW, and Windows Off/Acrylic backdrop.
- Home exposes compact Limits, Models, Activity, and Trend. Tools, Models, Sessions, and Limits use live normalized data. Unavailable providers stay out of Home Limits but remain inspectable in the dedicated Limits view.
- Model and Session share a provider filter toolbar outside the scrollable row list. Its dropdown remains an overlay and does not reflow the list; these two views use a compact low-visibility scrollbar so long lists retain position context without consuming meaningful width.
- Dynamic derived results use TTL eviction and a 64-entry bound. Session metadata reads only selected-period references in serial batches of 250, isolates batch failures, and caches at most 4,096 normalized session metadata records for 60 seconds. Pending request ownership is separate from result retention.
- Provider-owned session titles/project labels are allowed; fallback order is provider title → project/path basename → session id. Prompt/response-derived titles are prohibited.
- Codex/Claude session detail exposes only exchange structure, token/cache/reasoning buckets, tool names, timestamps, and proportional cost metadata.

### Floating Bubble, tray, and windows

- Floating Bubble defaults on. Retained modes are `limitsAllSessions`, `icon`, `barsSession`, `barsWeekly`, `barsAllSessions`, and `bars`.
- `limitsAllSessions` supports an independent provider selection. Empty selection keeps Auto (first two usable providers); explicit selection shows any 1–4 supported providers in stable Codex → Claude → Gemini → Antigravity order. Other bubble modes keep their prior semantics.
- Bubble height is fixed at 34 logical px and width is content-driven up to 320 logical px. Docking, dragging, resizing, and cross-monitor movement are DPI-aware.
- On Windows, Bubble may intentionally overlap the taskbar/reserved monitor area. While collapsed in that area, a narrow Win32 z-order keeper reasserts `HWND_TOPMOST` without moving/resizing/activating the window; recent Token Lens drag movement suppresses that keeper briefly so the existing pointer-driven drag path remains authoritative.
- Minimize uses Bubble → tray → OS-minimize fallback. Close always quits. Tray restore reconciles native/renderer bubble state before focusing the main window.
- Windows Acrylic is disabled while collapsed and restored on expansion when enabled. Token Lens-owned background subprocesses that should remain invisible use no-window hosting; Claude/Gemini auth-touch intentionally use hidden real consoles because ConPTY failed to refresh reliably on the validated Windows machine.

### Packaging and diagnostics

- Installed bundles use Tauri `externalBin` with pinned tokScale 4.15.1. Windows also ships a single-file portable EXE that embeds only compressed `tokscale.exe` and extracts it to an isolated temporary run directory.
- Windows packaging emits installer, portable EXE, and `SHA256SUMS.txt`; package helpers validate tokScale identity/version and PE expectations.
- Provider incident logs are sanitized JSONL, capped and retained for up to 3 days. Startup timing retains only the latest 10 launches and records no account/path/credential/provider payload content.
- tokScale failures expose only coarse failure kind/exit status to the renderer; raw stderr, malformed stdout excerpts, and executable paths are excluded.
- The 2026-09-18 audit changes pass 84 frontend tests, 132 Rust tests (9 live tests ignored), rustfmt, Clippy with warnings denied, the macOS native Tauri debug build, and npm audit (0 vulnerabilities). Windows CI/packaging and live UI/provider validation are separate gates, not inferred from the macOS run.

## Next action

B1–B4 are implemented. Complete the controllable DOM integration and build/CI portions of B5 from [the purpose audit](docs/purpose-audit-2026-09-18.md), retaining live Windows/provider gates separately. Keep packaged-runtime and visual validation as separate gates; do not redesign the UI or replace the current drag implementation. Preserve tokScale-first authority, provider cooldowns, no same-credential API re-probe during recovery, no Token Lens refresh-token redemption/credential writes, no inference solely for auth refresh, and strict content minimization.

## Known open items

No architecture decision currently blocks implementation.

External/live validation still worth retaining as explicit gates:

- Codex Business `individualLimit` on an account that actually exposes the value;
- AGY quota against a live Antigravity app/CLI/IDE source;
- final packaged Windows/macOS visual parity, including mixed DPI/multi-monitor behavior and Acrylic suspend/restore edge cases.

Deferred performance work:

- persistent tokScale extraction cache for Windows single-EXE remains low priority; measured extraction overhead on a tested Windows machine was small enough that it does not justify extra lifecycle complexity unless an endpoint-security environment demonstrates a material regression.

## Authoritative references

- [`docs/architecture/v2-architecture.md`](docs/architecture/v2-architecture.md) — durable product, security, provider-authority, and runtime contract.
- [`docs/migration/v1-porting-map.md`](docs/migration/v1-porting-map.md) — v1 behaviors that are deliberately preserved or excluded.
- Git history — chronology, completed experiments, benchmark runs, workflow IDs, and superseded implementation details.
