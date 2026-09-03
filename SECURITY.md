# Token Lens Security Model

Token Lens is a private downstream distribution of `Javis603/token-monitor` focused on local usage and quota visibility for Claude Code, Codex, and Antigravity.

## Security invariants

The following are product constraints, not user preferences:

1. Active usage clients are limited to `claude`, `codex`, and `antigravity`.
2. Active quota providers are limited to `claude`, `codex`, and `antigravity`.
3. Hub mode is always `local`; remote synchronization is disabled.
4. The embedded Hub HTTP server must not become reachable through persisted settings or environment variables.
5. Discord Rich Presence is disabled.
6. Runtime downloads of tokscale from npm are disabled. The bundled/pinned tokscale is used.
7. Application update checks, downloads, and installs are disabled in the runtime. Upstream updates are reviewed and integrated through Git.
8. Claude Web cookie storage is disabled. Claude Code OAuth/CLI discovery remains the intended Claude authentication path.
9. Session-detail token/timing/tool metadata may be retained, but `promptPreview` is redacted before crossing the session-detail resolver boundary.
10. New model identifiers are accepted through provider usage data; model names are not an execution allowlist.
11. Electron renderer isolation controls inherited from upstream (`contextIsolation`, disabled Node integration, CSP, navigation restrictions) must remain intact.

These invariants are regression-tested in `tests/downstream/securityPolicy.test.js`.

## Trust boundary

Token Lens still reads local usage/session metadata belonging to the enabled coding tools and uses provider authentication material already available to those tools when obtaining quota information. It therefore remains privileged developer tooling and must be treated accordingly on managed corporate endpoints.

The focused downstream policy reduces exposed features; it does not turn the application into a sandbox for untrusted local users or malware already running with the same OS account privileges.

## Upstream code

Provider implementations that are inactive in Token Lens can remain in the source tree so upstream fixes can be absorbed with low conflict. Runtime allowlists, rather than source-file absence, are the authoritative execution boundary.

Security-sensitive upstream changes must be reviewed before integration, especially changes to:

- `src/electron/main.js` and `preload.js`
- credential handling
- provider quota transports
- Antigravity local RPC/OAuth behavior
- tokscale packaging/updating
- Electron dependencies and BrowserWindow configuration
- new network listeners or synchronization features

## Reporting

This is a private personal downstream. Security findings should be tracked privately in this repository. Findings that also affect the public upstream should be disclosed to the upstream maintainer through an appropriate private channel when available, or a minimally revealing public issue if no sensitive exploit detail is required.
