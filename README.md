<p align="right">
  <strong>English</strong> | <a href="./README.ko.md">한국어</a>
</p>

# Token Lens

**A focused, security-hardened Windows desktop monitor for Codex, Claude Code, and Antigravity token usage and quotas.**

Token Lens is a private downstream distribution of [Javis603/token-monitor](https://github.com/Javis603/token-monitor). It keeps the upstream dashboard and usage-collection strengths while deliberately narrowing the runtime surface for a local Windows workflow.

> Token Lens is not a drop-in mirror of upstream Token Monitor. Unsupported providers and several upstream account, sync, updater, and credential-management features are intentionally disabled or hidden.

## Why this fork exists

The upstream project supports a broad set of AI tools and multi-device features. Token Lens instead optimizes for a smaller operating model:

- **Codex, Claude Code, and Antigravity only**
- model-level token accounting without a hardcoded model allowlist
- local usage history, quota/reset visibility, and floating desktop UI
- provider credentials remain owned by the original coding tools
- no Token Lens-managed account switching or credential persistence
- no multi-device Hub/sync, Discord RPC, in-app updater, or runtime tokscale download
- reduced transcript-content exposure: session detail keeps usage metadata, not prompt/response text

The result is a smaller and more conservative downstream contract intended for personal use on Windows, including managed/corporate endpoints where minimizing unnecessary attack surface matters.

## Supported tools

| Tool | Token usage | Quota / limits | Session metadata |
|---|:---:|:---:|:---:|
| Claude Code | ✅ | ✅ | ✅ |
| Codex | ✅ | ✅ | ✅ |
| Antigravity | ✅ | ✅ | — |

### Model handling

Token Lens filters **providers**, not model identifiers. New model names from Codex, Claude Code, or Antigravity remain visible in model-level usage attribution as long as the provider's underlying log/API schema remains compatible.

Unknown models can still be counted even when pricing metadata has not yet been added.

## Security posture

Token Lens is a monitor, not an authentication manager.

The downstream security contract includes:

- runtime provider allowlist: `claude`, `codex`, `antigravity`
- Hub/remote synchronization disabled
- embedded Hub listener disabled
- Discord Rich Presence disabled
- application auto-update disabled
- runtime tokscale download/update disabled
- no custom Token Lens credential store
- Codex authentication treated as read-only
- Claude Code authentication treated as read-only; Token Lens does not refresh/rotate Claude OAuth credentials
- no Token Lens-managed Antigravity OAuth accounts
- prompt previews disabled at the session parser boundary
- prompt/response text excluded from renderer session-detail payloads
- ancillary third-party network features disabled
- Electron isolation/CSP/navigation restrictions retained from upstream
- distinct Windows runtime/package identity: `Token Lens` / `com.teeeeooo.tokenlens`

These constraints are checked by the downstream invariant suite before CI acceptance and Windows packaging.

For the complete security model, see [SECURITY.md](./SECURITY.md).

## Authentication behavior

Token Lens reads authentication/session state already maintained by the supported coding tools when quota information requires it.

- **Codex:** use Codex CLI normally. Token Lens reads the existing state and does not write `auth.json`, perform managed login, or switch accounts.
- **Claude Code:** use Claude Code normally. Token Lens can read valid existing OAuth/CLI state but does not persist cookies or refresh OAuth credentials itself.
- **Antigravity:** Token Lens uses existing local Antigravity state/RPC where available and does not create a Token Lens-managed account.

If provider authentication expires, quota data may be unavailable until the corresponding coding tool refreshes its own authentication state.

## Data and privacy boundary

Token Lens reads local usage/session files for the three supported tools. Those files can contain sensitive developer information.

For Claude Code and Codex session detail, Token Lens retains only metadata needed for usage analysis, such as timestamps, token/cache counts, tool names, turn information, and cost attribution. Prompt previews are empty and prompt/response text is not retained in returned session-detail records or sent to the renderer.

Raw transcript files are still parsed locally in-process to derive that metadata. Token Lens should therefore be treated as privileged local developer tooling.

## Windows installation

Token Lens currently builds Windows x64 artifacts through GitHub Actions:

- `Token-Lens-Setup-<version>.exe` — installer
- `Token-Lens-<version>.exe` — portable executable
- `SHA256SUMS.txt` — artifact checksums

Open the repository's **Actions → Build Token Lens Windows** workflow and download the `token-lens-windows-x64` artifact from a successful `main` run.

### Unsigned builds

Token Lens downstream artifacts are intentionally **unsigned**. They do not use the upstream project's SignPath identity.

Windows SmartScreen, WDAC/AppLocker, EDR, or organization policy may warn about or block unsigned executables. On a managed endpoint, follow your organization's software and code-signing policy.

## Current build baseline

Token Lens currently tracks the upstream v0.53.0 code line, initially imported from:

```text
Javis603/token-monitor
v0.53.0
0b17b1ec53ccd60508a645144ccb7db74027168c
```

The downstream Git history preserves the upstream baseline so relevant provider fixes can be selectively integrated without turning Token Lens into an independent rewrite.

See [UPSTREAM.md](./UPSTREAM.md) for the maintenance procedure.

## Development

Requirements:

- Node.js 22
- npm
- Windows for producing the supported packaged artifacts

Install dependencies:

```bash
npm ci --prefer-offline --no-audit --no-fund
```

Apply downstream policy materialization:

```bash
node scripts/downstream/apply-hardening.js
node scripts/downstream/apply-provider-hardening.js
node scripts/downstream/apply-renderer-hardening.js
```

Run downstream invariants and lint:

```bash
node --test tests-downstream/security-invariants.test.js tests-downstream/branding-invariants.test.js
npm run lint
```

Production dependency audit:

```bash
npm audit --omit=dev --audit-level=high
```

The GitHub Actions workflows perform these checks before generating Windows installer/portable artifacts.

## Upstream relationship

Token Lens is based on the MIT-licensed [Javis603/token-monitor](https://github.com/Javis603/token-monitor) project and retains substantial upstream code and history.

The downstream policy is intentionally narrower. Upstream changes are reviewed before integration, with particular attention to:

- Codex / Claude Code / Antigravity collectors and quota APIs
- credential and authentication behavior
- Electron/preload/IPC security boundaries
- network listeners and synchronization features
- tokscale packaging and update behavior
- new model/log schema changes

Upstream workflows are not automatically trusted or imported into the downstream release path.

## License

MIT. See [LICENSE](./LICENSE).

Token Lens is an unofficial downstream project and is not affiliated with or endorsed by the upstream maintainer, OpenAI, Anthropic, or Google.
