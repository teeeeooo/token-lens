# Token Lens v2 — Current State

## Current phase

Token Lens v2 is now promoted to `main`; the previous Electron-based v1 line is preserved on `v1-legacy` at `493d4a5`. The v2 implementation and Windows package-shape automation are complete through promotion commit `983b764`, including the floating monitor, CSP hardening, retired-subsystem CSS cleanup, the roomier persisted window shell, and the read-only provider-quota parity follow-up. Real Windows QA after `e81291a` showed that Codex/Claude/Gemini usage aggregation worked while quota did not; commit `53315ae` restores the missing Codex OAuth, Claude Windows/WSL credential, and Gemini CLI current secure-storage paths without adding credential refresh/write behavior. Fresh macOS/Windows CI and Windows installer/single-EXE packaging are green on `main`; a real Windows live-provider retest remains the quota release gate.

## Accepted baseline

- v2 is an orphan lineage with a clean codebase; the previous v1 line is preserved on `v1-legacy` for reference.
- desktop runtime: Tauri 2.
- current supported provider set: Codex, Claude, Gemini CLI, Antigravity; future providers require an explicit adapter/security/data-contract review rather than a core redesign.
- tokScale is the primary actual-usage and supported-quota engine.
- Token Lens owns only confirmed tokScale/provider gaps: Codex read-only OAuth/App Server quota plus Business `individualLimit`, read-only Claude OAuth usage/spend enrichment, Gemini CLI quota, and AGY quota.
- established Token Lens UI/UX is preserved rather than redesigned.
- model and session usage remain core product features; Codex/Claude session usage/metadata detail remains in scope, while raw prompt/response content remains outside the renderer contract.
- raw tokScale/provider schemas stay behind a stable normalization boundary; provider-owned session titles are approved metadata, but transcript-derived title fallbacks are prohibited.

## Current implementation

- tokScale 4.15.1 remains the pinned v2 baseline;
- stable Rust domain types own normalized usage, history, quota, tokScale status, credit, and reset-credit payloads;
- the Rust tokScale adapter collects today/week/month/all-time usage with model or session grouping;
- quota normalization admits only Codex, Claude, Gemini CLI, and Antigravity and keeps canonical/additional lanes distinct;
- Codex reset credits, ordinary credit status, and scalar spend-control state are retained from tokScale;
- Codex base-quota enrichment now follows `tokScale → existing Codex OAuth usage endpoint → App Server`: healthy tokScale canonical windows always win, OAuth may fill missing base windows without refreshing/writing `auth.json`, and App Server may fill any lane still missing plus structured Business/Team/Enterprise `individualLimit`;
- Tauri commands expose normalized reports; raw tokScale JSON is not a renderer contract;
- `window.tokenMonitor.getStats` composes the retained local stats contract and preserves serial tokScale scans;
- the compatibility payload retains client/model/session totals, token components, message counts, provider attribution, and cost;
- Codex reasoning follows the v1 additive public-total convention without double-counting Claude/Gemini/AGY reasoning;
- the renderer preserves the Token Lens stylesheet/class vocabulary, icon language, frameless transparent geometry, and always-on-top behavior rather than the temporary skeleton UI; first launch now uses 380x720 logical pixels, expanded resizing is bounded to 300x140 through 1200x1400, and the last normal expanded size/position is persisted without allowing floating-bubble geometry to overwrite it; retired Hub/export/diagnostic/service-status/updater/account-switch/session-archive CSS has been removed while provider icon/style assets remain available for later explicit provider adapters;
- the Tauri WebView now restores the v1 defense-in-depth CSP boundary: scripts are self-only, Tauri IPC is the only non-self renderer connection surface, object/base/form/frame surfaces are blocked, and only inline style mutation remains narrowly allowed for the retained dynamic visual renderer;
- the retained appearance subset is implemented: the three built-in Default/Obsidian/Porcelain presets, persisted 70–160% zoom, optional compact total tokens using fixed `K/M/B` units, OS-driven `prefers-reduced-motion`, always-on live/tool indicators, and a fixed system UI font stack (`system-ui`, Segoe UI on Windows); font-selection UI, custom colors, theme codes, and layout swapping remain intentionally excluded;
- interface language now defaults to `Auto (system)` and retains English, Korean, Japanese, Simplified Chinese, and Traditional Chinese for the v2-visible surface; locale-sensitive dates and week boundaries follow the resolved system/user locale;
- Windows backdrop settings are narrowed to `Off` / `Acrylic` with Acrylic default-on; the native Tauri window-effects API owns Acrylic, floating-bubble collapse clears it, expansion restores it when enabled, and effect-application failure is deliberately non-fatal rather than falling back to the v1 experimental Accent Blur path;
- Home restores Limits, Models, and long-range Activity/Trend modules; its compact Limits module now shows only providers with usable quota/billing windows as v1 did, while the dedicated Limits view keeps unavailable providers inspectable; Tools, Models, Sessions, and Limits detail views remain wired to live `getStats` data;
- dashboard history is sourced directly from `tokscale graph` through a normalized `HistoryReport` and retained `getDashboardHistory` facade rather than the v1 persisted history subsystem; the rolling-year heatmap and 45-point trend patch today's bucket from live stats and preserve horizontal scroll position across refreshes;
- DAY / MONTH / TOTAL switching, manual refresh, view switching, close/minimize, drag region, and floating/normal pin toggle are wired through Tauri; the period selector sits in the true center of a symmetric three-column titlebar, its moving indicator is clipped inside the selector chrome, and the right-side window controls retain a separate full hover region so neither surface overlaps the other;
- the MONTH slot now preserves v1 `MONTH` / `WEEK` / `7D` / `30D` selection semantics, including locale-aware first-day-of-week behavior;
- derived Week / Last 7 / Last 30 ranges use a narrow `--since YYYY-MM-DD` tokScale command rather than reintroducing the v1 history subsystem;
- stats polling is visibility-aware at 30 seconds, while the compatibility layer caches Today (30s), Month (2m), derived ranges (1m), All Time (5m), and quota (5m); manual refresh bypasses all caches;
- overlapping renderer refreshes are serialized and coalesced so period changes cannot race an in-flight tokScale scan;
- the renderer controller is a small v2-specific implementation rather than a port of the v1 788 KB `app.js`;
- macOS transparent-window support is enabled through Tauri's `macos-private-api`; this implies macOS App Store distribution is not a target for this configuration;
- the persisted settings store now owns the retained floating/tray and appearance controls; fresh installs default Floating Bubble on with `limitsAllSessions`, while existing persisted v2 choices remain preserved, and the accepted bubble modes are limited to provider limits, icon-only, lowest session, lowest weekly, first-two-provider bars, and lowest-remaining bars;
- floating-bubble collapse/expand, left/right edge docking, drag-to-cursor movement, skip-taskbar behavior, always-on-top restoration, and original collapsed renderer classes are implemented through native Tauri window APIs;
- floating-bubble geometry is DPI-aware with a fixed 34px logical height and content-driven logical width; docking, dragging, resizing, and movement between monitors recalculate physical dimensions at the destination scale factor;
- Windows-specific bubble policy is explicit: collapse against full monitor bounds with zero edge margin, while macOS/other desktop targets use the work area with the original vertical margin;
- Windows background subprocesses owned by Token Lens (tokScale, Codex App Server, and AGY/PowerShell discovery helpers) use `CREATE_NO_WINDOW`, preventing periodic refresh from flashing CMD/PowerShell windows;
- macOS-hosted `x86_64-pc-windows-msvc` checks remain supplemental only: native C dependencies such as the bundled SQLite used for read-only Codex title metadata require a Windows CRT/SDK that is not present on the Mac host, so the authoritative Windows compile/build gate is the GitHub `windows-latest` native job;
- local macOS native runtime smoke verifies the frameless main window plus actual collapse, native move, and expansion transitions; temporary smoke settings/source instrumentation were removed afterward;
- the current local gate is green for the window-shell and quota-parity follow-ups: `npm run check` (57/57 frontend tests; 68 Rust tests passed with 9 live tests ignored), Clippy with `-D warnings`, rustfmt, native macOS Tauri debug build, `npm audit` with 0 vulnerabilities, and `git diff --check` all pass; Codex OAuth live smoke succeeds on the current Mac account;
- commit `53315ae` is green in `Token Lens v2 CI` run `34078935973` on both macOS and Windows (Windows: 69 Rust tests passed with 9 live tests ignored), and `Build Token Lens v2 Windows` run `34078935933` completed packaging-helper checks, pinned tokScale 4.15.1 staging, unsigned NSIS/single-EXE build, and artifact upload successfully;
- the retained native tray shell is restored with default-on visibility, the original macOS template icon, Today-token menu-bar title where supported, usage/cost tooltip, left-click focus/restore, Refresh Now, retained-view navigation, Settings, version, and Quit actions;
- window controls now use explicit v2 semantics: minimize collapses to Floating Bubble when enabled, otherwise hides to the tray when available and finally falls back to OS minimize; close always quits, and ordinary focus loss no longer collapses the window;
- the renderer listens for narrow tray actions rather than reintroducing Electron IPC, and publishes only the small Today usage/cost summary needed by the native tray;
- the v2 settings surface now controls tray visibility as well as floating-bubble behavior;
- session-list identification now uses a separate read-only provider-metadata adapter rather than transcript text: Codex resolves provider-owned `display_title` / `title` from local Codex SQLite state/catalog data, while Claude resolves explicit `aiTitle` metadata from its JSONL session files;
- renderer session-title fallback is provider-owned title → project/path basename → session identifier, with client/model/message information kept as secondary context; full working-directory paths and raw prompt/response fields are not part of the normalized metadata payload;
- session metadata is requested only while the Sessions view is active, then batch-enriched and cached independently from tokScale usage so metadata lookup failure cannot take down usage/quota reporting; payload-shape regression tests and a local ignored live smoke enforce the privacy boundary;
- Codex/Claude per-turn session usage/metadata detail is now restored through a small Rust parser invoked with `spawn_blocking`; it resolves only the selected local session file and emits timestamps, exchange/turn structure, token/cache/reasoning buckets, tool names, and proportional cost attribution;
- the detail parser deserializes only structural/usage/tool metadata, never prompt/response text or previews; Claude `isMeta` and `tool_result` records do not create user-exchange boundaries, while duplicate Claude message blocks are de-duplicated by message id/line uuid;
- Sessions rows for Codex/Claude now drill into the retained detail surface with neutral `Exchange #N` / `Reply #N` labels, newest/most-tokens sorting, expandable turn rows, and current-period filtering using an absolute local-calendar range start;
- Codex raw reasoning is normalized into the same disjoint additive bucket used by the v2 tokScale compatibility contract, while Claude retains its provider output semantics;
- Codex App Server enrichment is a best-effort supplement to healthy tokScale quota data: Token Lens captures the selected local Codex workspace before the tokScale read, requires that workspace to remain unchanged across the RPC probe, compares account email when both sides expose one, and discards the enrichment on any mismatch or App Server failure;
- the App Server probe runs `codex -s read-only -a untrusted app-server`, calls only `account/read` with `refreshToken: false` plus `account/rateLimits/read`, fills missing canonical primary/secondary quota only when tokScale has no usable base window, and materializes Business Monthly `CREDITS` only when tokScale has no structured absolute credit window;
- Claude quota parity now uses an isolated read-only `/api/oauth/usage` adapter only when tokScale is missing required data; it may fill missing 5-hour/weekly windows and the monetary `Usage credits` spend/extra-usage lane, while preserving existing tokScale windows and provider plan labels such as `Enterprise zero` verbatim;
- Claude credential access reuses only provider-owned access-token state from env/native-file storage; on Windows it also rediscovers provider-owned WSL `~/.claude/.credentials.json` files through `\\wsl$` and may read Windows Credential Manager. If the credential is missing or remains rejected after re-read, Token Lens schedules one bounded bare `claude` startup in the background (60 s hard timeout, 1 s credential polling), immediately returns the current stale/unavailable quota, discards all TUI output, and lets Claude Code own any refresh/write. Only one Claude refresh runs at a time, and the next quota poll consumes the refreshed provider credential; Token Lens never reads/redeems a Claude refresh token or writes credentials itself;
- the retained Limits renderer preserves capability-based absolute quota metadata and displays Business monthly credits plus Claude monetary usage without provider-specific presentation hacks;
- local live smoke confirms both the installed Codex App Server transport/schema path and the restored Codex OAuth `/wham/usage` fallback; an actual Business account is still required to validate live `individualLimit` values end to end, and fresh Claude credentials remain an external quota validation gate;
- Gemini CLI actual usage is now admitted directly from tokScale as a first-class `gemini` client/provider; the renderer keeps Gemini CLI distinct from Antigravity while Google/Gemini model rows use the Gemini visual identity;
- Gemini session metadata uses only the local session header `sessionId`, its provider-owned `tmp/<project-key>` location, and `projects.json` path-to-key mapping, exposing only the project basename and never deriving a title from conversation content;
- Gemini quota uses a best-effort read-only `loadCodeAssist` → `retrieveUserQuota` adapter from provider-owned access tokens; on Windows it reads `gemini-cli-oauth/main-account` from Credential Manager and the current Gemini CLI encrypted `~/.gemini/gemini-credentials.json` file-keychain fallback, with older `~/.gemini/oauth_creds.json` retained for migration compatibility. If the credential is missing, expired, or rejected, Token Lens schedules one bounded bare `gemini` startup in the background (120 s hard timeout, 1 s credential polling), immediately returns the current stale/unavailable quota, discards all TUI output, and lets Gemini CLI own any refresh/write. Only one Gemini refresh runs at a time, and quota polling temporarily tightens to 30 s until provider recovery is observed; Token Lens never reads/redeems Google refresh tokens, writes credentials itself, or calls onboarding/project-creation APIs;
- Gemini model quota windows remain allowlist-free and Limits shows all returned model buckets, while Home selects the two lowest remaining percentages;
- AGY quota now uses an isolated local-language-server adapter: it detects running Antigravity app, `agy`/CLI, and IDE processes in that order, probes only `127.0.0.1`, prefers grouped `RetrieveUserQuotaSummary`, and falls back to `GetUserStatus` / `GetCommandModelConfigs` without reintroducing the v1 quota framework;
- AGY grouped local quota preserves Gemini and Claude/GPT 5-hour/weekly lanes; legacy local or remote model-only payloads remain conservative family windows and do not invent cadence;
- the AGY remote OAuth path is deliberately only an explicit already-valid credential-snapshot seam (`ANTIGRAVITY_OAUTH_CREDENTIALS_FILE`); Token Lens does not log in, refresh, persist, onboard, or read another monitor's credential store;
- no Antigravity installation/process or provider-owned credential source is present on the current Mac, so live AGY quota values remain an external validation gate;
- a native macOS debug `.app` bundle contains `Contents/MacOS/tokscale` beside `token-lens`, and the packaged sidecar reports exactly `tokscale 4.15.1`; Windows NSIS keeps the same adjacent Tauri `externalBin` layout, while the no-install artifact now restores the v1 single-file UX by appending a gzip-compressed pinned `tokscale.exe` payload to the normal Tauri GUI executable;
- portable tokScale extraction is isolated under the system temp directory, guarded by an active-run lock, removed on normal adapter/process teardown, and followed by best-effort cleanup of unlocked stale run directories on a later portable launch; explicit `TOKEN_LENS_TOKSCALE_BIN` remains the only higher-priority developer/test override;
- the packaging helper emits `Token-Lens-Setup-<version>.exe`, `Token-Lens-<version>.exe`, and `SHA256SUMS.txt`, verifies the original sidecar is exactly tokScale 4.15.1, and rejects signed or non-GUI PE output for the installer, normal app executable, and single-file portable; the native Windows packaging workflow and Windows-only active-lock cleanup test have already passed for the single-EXE package shape, so the remaining release gate is runtime/visual validation on a real Windows machine.

## Next action

Validate the fresh `53315ae` Windows package on the target Windows machine; automated implementation and packaging gates are green.

1. install/run the NSIS artifact from `Build Token Lens v2 Windows` run `34078935933` and confirm adjacent bundled tokScale 4.15.1 resolution;
2. run the single-file portable artifact from the same run and confirm embedded-portable tokScale resolution/temp cleanup plus absence of CMD/PowerShell flashes;
3. validate live Codex Business quota/credits, Claude Enterprise monetary quota, Gemini CLI quota, and the retained Acrylic/Bubble/window/titlebar behavior.

Current `53315ae` Windows artifact checksums:

- `Token-Lens-Setup-2.0.0-alpha.0.exe`: `3930f3fa1be591a03be93d7184a3e25bc28f54d1d9040dd7d20237630ea09207`;
- `Token-Lens-2.0.0-alpha.0.exe`: `95b2466d8496e0db83d388218e07d543e915139b019d1bc589452594991395c8`.

The downloaded artifact independently matches `SHA256SUMS.txt`; both outer executables are unsigned Windows GUI PEs, and the portable footer/payload round-trip verifies `TLTS0001` with the expected 24,359,424-byte embedded tokScale payload.

Antigravity remote OAuth remains conditional: wire it only if an Antigravity-owned credential source is independently confirmed; do not add a Token Lens-managed OAuth login/store framework. The v1 token/cost text modes and custom tray/bubble composer remain intentionally excluded; only the six accepted compact quota modes are retained.

## Known open items

No architecture decision currently blocks implementation.

Production tokScale packaging is wired through Tauri `externalBin` for installed bundles and the single-file portable overlay for no-install Windows use. The build stages the platform-native 4.15.1 npm binary under the required target-triple name, validates package identity plus `--version`, and portable runtime discovery prefers its embedded compressed payload before adjacent/project/PATH fallbacks. The hardened no-runtime-download/update policy is unchanged.

The final retained-facade audit found no reason to recreate v1 Electron-only push/utility APIs for unreachable UI. The current renderer calls only the implemented settings/stats/history/session/floating/tray surface; minimize uses a narrow Tauri command because it owns the Bubble → tray → OS fallback policy, while close/drag/pin use native Tauri window behavior directly. Hardened v1 app-update/download/account-mutation/diagnostic/export surfaces remain absent by policy.

External/runtime validation still required:

- Claude quota against a fresh valid Claude credential;
- Codex Business `individualLimit` end-to-end validation on an actual Business account;
- Gemini CLI quota end-to-end validation while the provider-owned OAuth access token is already valid;
- AGY quota against a live Antigravity app/CLI/IDE source, plus automatic remote fallback only if an Antigravity-owned credential source is confirmed;
- packaged tokScale runtime resolution after launching the installed/portable Windows artifact;
- native Windows Acrylic plus floating-bubble validation at common DPI scales (100/125/150%), multi-monitor movement, taskbar/skip-taskbar behavior, transparent mini-window chrome, and Acrylic suspend/restore across collapse/expansion;
- target-Windows parity for the latest fixes: `Auto (system)` Korean UI/system font, Home unavailable-provider filtering, titlebar control hit areas, no background CMD/PowerShell flashes, and live Codex/Claude/Gemini quota against the installed provider-owned credentials;
- final macOS/Windows visual parity on packaged artifacts.

## Authoritative references

- [`docs/architecture/v2-architecture.md`](docs/architecture/v2-architecture.md)
- [`docs/migration/v1-porting-map.md`](docs/migration/v1-porting-map.md)
