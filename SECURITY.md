# Token Lens Security Model

Token Lens is a private downstream distribution of `Javis603/token-monitor` focused on local usage and quota visibility for Claude Code, Codex, and Antigravity.

## Security invariants

The following are product constraints, not user preferences:

1. Active usage clients are limited to `claude`, `codex`, and `antigravity`.
2. Active quota providers are limited to `claude`, `codex`, and `antigravity`.
3. Hub mode is always `local`; remote synchronization is disabled.
4. The embedded Hub HTTP server cannot be enabled through persisted settings or environment variables.
5. Discord Rich Presence is disabled.
6. Runtime downloads/updates of tokscale from npm are disabled. The bundled/pinned tokscale is used.
7. Application update checks, downloads, and installs are disabled in the runtime. Upstream updates are reviewed and integrated through Git.
8. Token Lens does not create or consume its own credential store. Sensitive provider credentials are stripped from Token Lens settings.
9. Existing Codex authentication is read-only to Token Lens: managed login/account switching is not exposed and direct writes to Codex `auth.json` are rejected.
10. Existing Claude Code authentication is read-only to Token Lens. Claude Web cookie storage is disabled and Token Lens does not refresh/rotate Claude OAuth credentials; expired authentication must be refreshed by normal Claude Code use.
11. Token Lens does not create managed Antigravity OAuth accounts. Existing local Antigravity state/RPC may be read for usage/quota collection.
12. Session detail is limited to Claude and Codex token/timing/tool metadata. Prompt previews are empty at their source; prompt and response text are not copied into returned event/exchange records or renderer IPC payloads.
13. Raw local transcript files are necessarily read and parsed in-process to derive usage metadata. Token Lens does not persist or transmit their prompt/response content.
14. New model identifiers are accepted through provider usage data; model names are not an execution allowlist.
15. Renderer-accessible privileged IPC is reduced to the focused product surface; disabled provider login, credential, update and third-party status operations are not exposed through preload.
16. Ancillary third-party network features such as exchange-rate refresh and Codex reset-forecast lookup are disabled. Network access remains only where needed by the three enabled providers and user-directed safe external links.
17. Electron renderer isolation inherited from upstream (`contextIsolation`, disabled Node integration, restrictive CSP and navigation controls) must remain intact.
18. Windows package/runtime identity is distinct from upstream: `Token Lens` / `com.teeeeooo.tokenlens`.

These invariants are regression-tested in `tests-downstream/security-invariants.test.js` and run before both downstream CI acceptance and Windows packaging.

## Authentication behavior

Token Lens is a monitor, not an authentication manager. The supported operating model is:

- Codex: read the authentication/session state already maintained by Codex CLI; do not log in, switch accounts, or modify `auth.json` from Token Lens.
- Claude Code: read existing OAuth/CLI state for quota access while valid; do not persist cookies or refresh/rotate OAuth credentials from Token Lens.
- Antigravity: use existing local Antigravity state/RPC where available; do not create Token Lens-managed OAuth accounts.

If provider authentication is expired or unavailable, Token Lens may report quota as unavailable until the corresponding coding tool refreshes its own state.

## Session-content boundary

Session-detail parsing exists only to derive usage metadata such as timestamps, token counts, cache counts, reasoning counts where supplied, tool names, turn counts, and aggregate cost attribution. Claude/Codex prompt boundaries are represented internally without retaining the prompt text, and `promptPreview` is always empty.

The parser still opens local transcript files and parses their records in process. Therefore Token Lens remains privileged local developer tooling; the privacy guarantee is that transcript content is not retained in Token Lens records, persisted by Token Lens, sent to the renderer, or synchronized/transmitted by Token Lens.

## Trust boundary

Token Lens reads local usage/session metadata belonging to the enabled coding tools and uses provider authentication material already available to those tools when obtaining quota information. It must therefore be treated as privileged developer tooling on managed corporate endpoints.

The focused downstream policy reduces exposed features; it does not turn the application into a sandbox for untrusted local users or malware already running with the same OS account privileges.

## Upstream code

Provider implementations that are inactive in Token Lens can remain in the source tree so upstream fixes can be absorbed with low conflict. Runtime allowlists and the constrained preload/policy layer, rather than source-file absence, are the authoritative execution boundary.

Security-sensitive upstream changes must be reviewed before integration, especially changes to:

- `src/electron/main.js` and `preload.js`
- `src/shared/limitCollector.js` and provider authentication behavior
- credential handling
- provider quota transports
- Antigravity local RPC/OAuth behavior
- tokscale packaging/updating
- Electron dependencies and BrowserWindow configuration
- new network listeners or synchronization features

## Reporting

This is a private personal downstream. Security findings should be tracked privately in this repository. Findings that also affect the public upstream should be disclosed to the upstream maintainer through an appropriate private channel when available, or a minimally revealing public issue if no sensitive exploit detail is required.
