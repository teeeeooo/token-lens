# Token Lens

**A focused desktop monitor for AI coding-tool token usage and account quota.**

Token Lens v2 is a security-conscious downstream application derived from [Javis603/token-monitor](https://github.com/Javis603/token-monitor). It preserves the established Token Lens dashboard and floating-monitor workflow while replacing the Electron backend with a smaller Tauri 2 architecture centered on tokScale.

Current supported tools:

- Codex
- Claude Code
- Gemini CLI
- Antigravity (AGY)

The provider set is an explicit runtime allowlist, not a permanent architectural ceiling. Future providers may be added through a reviewed adapter without redesigning the normalized domain or renderer.

## Product goals

- preserve the Token Lens dashboard, limits view, activity charts, model/session exploration, tray behavior, and floating monitor UX;
- show reliable current, short-window, weekly, reset, and provider-specific quota information;
- show actual usage by day/week/month/all-time, including model/session breakdown, token categories, and cost when available;
- keep authentication state owned by the original coding tools;
- remain substantially smaller and simpler than the Electron-based v1 application.

## Architecture

Token Lens v2 uses Tauri 2 for the desktop shell and pins tokScale as the primary usage/quota data engine. Token Lens-owned provider code is limited to confirmed tokScale gaps such as Codex Business `individualLimit`, Gemini CLI quota, and AGY quota.

Raw tokScale/provider payloads stay behind stable Rust domain contracts before reaching the renderer. The renderer keeps only the compatibility surface required by the retained v2 UX rather than recreating the old Electron IPC framework.

For the authoritative system contract, see [`docs/architecture/v2-architecture.md`](docs/architecture/v2-architecture.md). For current engineering state and remaining validation, read [`STATE.md`](STATE.md) first.

## Security and privacy

Token Lens is a monitor, not an authentication manager.

- runtime providers are limited to Codex, Claude, Gemini CLI, and Antigravity;
- model identifiers are not hardcoded into an allowlist;
- provider credentials remain owned by the original coding tools;
- Token Lens does not provide general account switching or maintain its own credential store;
- runtime tokScale download/update, Hub/multi-device sync, Discord RPC, updater, export, diagnostics/service-status, and broad provider-management frameworks are excluded;
- renderer-facing session data excludes prompt/response text, content previews, reasoning summaries, and transcript-derived title fallbacks;
- provider-owned titles and project/path basenames may be used for session identification;
- the Tauri WebView runs with an explicit CSP: packaged scripts only, Tauri IPC as the only non-self connection surface, and a narrow inline-style exception for the retained dynamic renderer.

Local session files may still be parsed in-process to derive approved usage metadata, so Token Lens should be treated as privileged local developer tooling.

## Windows distribution

Windows x64 packaging emits:

```text
Token-Lens-Setup-<version>.exe   # NSIS installer
Token-Lens-<version>.exe         # single-file no-install portable
SHA256SUMS.txt
```

Windows release packaging uses Tauri NSIS plus the pinned tokScale native sidecar. The build validates package identity and the sidecar's reported version, and Token Lens does not download or update tokScale at runtime.

The portable EXE keeps the normal Tauri application executable as the outer GUI PE and appends a compressed `tokscale.exe` payload. At runtime it extracts only tokScale under the system temp directory, holds an active-run lock, removes the run directory on normal exit, and clears unlocked stale run directories on a later launch.

Artifacts are intentionally unsigned. Windows SmartScreen, WDAC/AppLocker, EDR, or organization policy may warn about or block them; managed endpoints should follow the applicable local software policy.

## Development

Requirements:

- Node.js 22+
- Rust stable toolchain
- platform prerequisites required by Tauri 2

Install locked dependencies and run the local validation suite:

```bash
npm ci
npm run check
```

Run the desktop app during development:

```bash
npm run dev
```

On Windows, generate release artifacts under `dist-v2/` with:

```bash
npm run package:windows
```

## Upstream relationship

Token Lens originated as a downstream of the MIT-licensed [Javis603/token-monitor](https://github.com/Javis603/token-monitor) project. The Electron-based v1 line initially tracked upstream Token Monitor v0.53.0 at commit `0b17b1ec53ccd60508a645144ccb7db74027168c`.

v2 is intentionally different from a normal source-tree fork: it is an orphan lineage and does not routinely merge upstream Token Monitor commits. The v1 lineage remains historical/reference evidence for proven UI behavior, provider edge cases, and security decisions.

Relevant upstream changes should still be reviewed selectively, especially when they reveal changes in provider log/API schemas, quota semantics, model/session attribution, or other behavior that may affect Token Lens. Upstream workflows or broad feature additions are not automatically imported into the v2 release path.

For v2 maintenance, tokScale is the primary evolving dependency for actual usage and supported subscription-quota behavior. Each Token Lens release pins and validates a tokScale version; provider-specific Token Lens adapters remain deliberately narrow so external changes have a small maintenance surface.

In short:

- **Token Monitor upstream** is a semantic/reference source, not a routine merge source for v2;
- **Token Lens v1** is the historical implementation reference;
- **tokScale** is the primary evolving usage/quota dependency;
- **Token Lens-owned adapters** cover only confirmed gaps and are reviewed independently.

## Documentation

Start with [`STATE.md`](STATE.md) for current status, then [`docs/README.md`](docs/README.md) for documentation routing. The durable v2 architecture contract is [`docs/architecture/v2-architecture.md`](docs/architecture/v2-architecture.md).

## License

MIT. See [`LICENSE`](LICENSE).

Token Lens is an unofficial downstream project and is not affiliated with or endorsed by the upstream maintainer, OpenAI, Anthropic, or Google.
