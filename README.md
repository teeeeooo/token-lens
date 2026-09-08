<div align="center">
  <img src=".github/assets/app.png" alt="Token Lens logo" width="120">
  <h1>Token Lens</h1>
</div>

<p align="center">
  <em>A lightweight, privacy-focused desktop monitor for AI coding-tool token usage and account quotas.</em>
</p>

<p align="center">
  <a href="https://tauri.app/"><img src="https://img.shields.io/badge/Tauri-2-blue.svg?logo=tauri&logoColor=white" alt="Built with Tauri 2"></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Rust-Stable-orange.svg?logo=rust&logoColor=white" alt="Powered by Rust"></a>
  <img src="https://img.shields.io/badge/Windows-x64-0078D4.svg?logo=windows&logoColor=white" alt="Windows x64">
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-yellow.svg" alt="License: MIT"></a>
</p>

<div align="center">
  <img src=".github/assets/demo.gif" alt="Token Lens dashboard demo" width="520">
</div>

Token Lens gives you a compact view of AI coding-tool usage, remaining quotas, reset times, model usage, and session activity without turning the monitor itself into an account manager.

It currently supports **Codex, Claude Code, Gemini CLI, and Antigravity (AGY)**. Token Lens v2 replaces the Electron backend used by v1 with a smaller **Tauri 2 + Rust** architecture and uses **tokScale** as the primary usage/quota data engine.

---

## ✨ Key Features

- 🪟 **Floating Monitor & System Tray** — minimize Token Lens into a draggable provider-quota bubble, or reopen it from the native tray. The default bubble shows provider icons with remaining quota percentages.
- ⏱️ **Quota & Reset Visibility** — monitor available short-window, weekly, reset, credit, and provider-specific limits with periodic background refresh, bounded stale-data fallback, and provider-level rate-limit cooldowns.
- 📊 **Usage & Cost Analytics** — inspect Day, Week, Month, Last 7 Days, Last 30 Days, and All-Time usage with model/session breakdowns, token categories, and cost where available.
- 🧭 **Session Exploration** — identify sessions using provider-owned metadata and inspect per-turn usage/tool metadata for supported providers without turning Token Lens into a transcript viewer.
- ⚡ **Tauri 2 + tokScale** — a focused native desktop shell with a deliberately smaller backend/runtime surface than the Electron-based v1 line.
- 🔒 **Privacy-Focused Data Boundary** — prompt/response content is not exposed to the renderer or displayed by Token Lens.

---

## Showcase

<table>
<tr>
<td width="33%" align="center"><img src=".github/assets/home-view.png" width="300" alt="Token Lens Home view"><br><sub>Home — quota status, model mix, activity heatmap, and recent trend</sub></td>
<td width="33%" align="center"><img src=".github/assets/limits-view.png" width="300" alt="Token Lens Limits view"><br><sub>Limits — provider quota windows, reset times, and available credits</sub></td>
<td width="33%" align="center"><img src=".github/assets/models-view.png" width="300" alt="Token Lens Models view"><br><sub>Models — token usage and cost aggregated by model</sub></td>
</tr>
<tr>
<td width="33%" align="center"><img src=".github/assets/sessions-view.png" width="300" alt="Token Lens Sessions view"><br><sub>Sessions — provider-owned titles/project labels with usage metadata</sub></td>
<td width="33%" align="center"><img src=".github/assets/session-detail.png" width="300" alt="Token Lens Session Detail view"><br><sub>Session Detail — exchange, token, cache, reasoning, and tool metadata</sub></td>
<td width="33%" align="center"><img src=".github/assets/floating-monitor.png" width="300" alt="Token Lens floating monitor"><br><sub>Floating Monitor — minimize into a compact provider-quota bubble</sub></td>
</tr>
</table>

<sub>Showcase images use synthetic mock data rendered with the v2 UI on macOS. Windows uses the same renderer with its platform-specific Acrylic backdrop option.</sub>

---

## 🔌 Supported Providers

| Provider | Usage | Quota & Limits | Session Aggregation | Session Metadata | Per-Turn Detail |
| :--- | :---: | :---: | :---: | :---: | :---: |
| **Codex** | ✅ | ✅ | ✅ | ✅ | ✅ |
| **Claude Code** | ✅ | ✅ | ✅ | ✅ | ✅ |
| **Gemini CLI** | ✅ | ✅ | ✅ | ✅ | — |
| **Antigravity (AGY)** | ✅ | ✅ | ✅ | tokScale-provided metadata only | — |

The provider set is an explicit runtime allowlist, not a permanent architectural ceiling. New providers may be added through a reviewed adapter and security/data-contract update; model identifiers themselves are not hardcoded into an allowlist.

### Quota recovery behavior

- **Claude Code** — normal tokScale/direct OAuth quota collection remains the primary path. On authentication failure Token Lens re-reads the provider-owned credential and retries once only if the access token changed. If recovery is still needed, it may use an interactive Claude CLI PTY `/usage` probe; non-interactive `claude -p "/usage"` is never treated as quota data. Enterprise credit/spend panels are supported by the fallback parser.
- **Gemini CLI** — Token Lens uses the existing read-only Gemini Code Assist adapter and re-reads the provider-owned access token after HTTP 401, retrying once only when the token changed. Token Lens does not use Gemini CLI `/stats model` subprocess/PTY scraping because current non-interactive behavior can fall through to an LLM prompt instead of returning authoritative quota data.
- **Rate limits and transient failures** — HTTP 429 respects `Retry-After` with a provider-level cooldown. A recent last-good quota may be shown explicitly as **Stale** for a bounded period; expired/reset-past windows are not silently reused.

---

## 🚀 Getting Started on Windows

Token Lens v2 currently produces pre-built Windows x64 artifacts through [GitHub Actions](https://github.com/teeeeooo/token-lens/actions/workflows/v2-build-windows.yml). Official GitHub Releases publication is not yet part of the v2 workflow.

From a successful **Build Token Lens v2 Windows** run, download the `token-lens-v2-windows-x64` artifact. It contains:

| Package | File | Description |
| :--- | :--- | :--- |
| **Installer** | `Token-Lens-Setup-<version>.exe` | Standard unsigned NSIS installer. |
| **Portable** | `Token-Lens-<version>.exe` | Single-file, no-install executable. The app extracts only its embedded tokScale sidecar to an isolated temporary run directory. |
| **Checksums** | `SHA256SUMS.txt` | SHA-256 hashes for the Windows executables. |

### Windows SmartScreen / endpoint protection

Token Lens Windows artifacts are currently **unsigned**. Windows SmartScreen, WDAC/AppLocker, EDR, or organization policy may warn about or block them.

On a personal machine, Windows may offer **More info → Run anyway** for an unsigned application. On managed or corporate endpoints, follow your organization's software and code-signing policy instead of bypassing local controls.

You can compare the downloaded files against `SHA256SUMS.txt` before running them.

### Portable behavior

The portable EXE keeps the normal Token Lens GUI executable as the outer PE and appends a compressed, pinned `tokscale.exe` payload. At runtime Token Lens extracts **only tokScale** under the system temporary directory, holds an active-run lock, removes that run directory on normal teardown, and performs best-effort cleanup of unlocked stale run directories on a later portable launch.

Token Lens does not download or update tokScale at runtime.

---

## 🔒 Security & Privacy

Token Lens is a **monitor**, not an authentication manager or transcript viewer.

1. **No Token Lens credential store** — Token Lens does not own provider accounts, provide general account switching, or refresh provider OAuth credentials. It consumes only existing provider-owned state or explicitly supplied read-only credential snapshots where the architecture permits them.
2. **Content-minimized session processing** — local provider session files may be read in-process to derive approved metadata such as timestamps, token/cache counts, tool names, turn structure, provider-owned titles, and project labels. Prompt/response text, content previews, reasoning summaries, and transcript-derived title fallbacks are excluded from the renderer-facing data model.
3. **Restricted WebView surface** — the Tauri frontend runs under an explicit Content Security Policy. Packaged scripts are self-only, arbitrary external WebView connections are not allowed, and only the inline-style compatibility required by the retained renderer is permitted.
4. **Narrow provider networking** — when quota data requires it, Rust backend adapters may contact the relevant official provider API or a detected local provider service. Token Lens does not provide a general-purpose network client surface to the renderer.
5. **No Token Lens cloud service** — no Hub/multi-device sync, Discord RPC, Token Lens telemetry backend, in-app updater, or silent runtime tokScale download/update is part of the v2 product.
6. **Provider-owned authentication lifecycle** — Token Lens does not redeem provider refresh tokens or write provider credentials. Recovery is limited to re-reading provider-owned access tokens and using provider-owned interfaces where explicitly supported.
7. **Error-only local diagnostics** — provider incidents such as authentication failures, rate limits, transport errors, and recovery outcomes are written as sanitized JSONL records. Tokens, credential payloads, authorization headers, raw CLI output, account identifiers, prompts, and transcripts are excluded. Logs are retained for up to **3 days**, capped at **1 MiB** total, and deduplicated for repeated incidents.

On Windows, diagnostic files are stored under:

```text
%LOCALAPPDATA%\com.teeeeooo.tokenlens\logs
```

You can open the exact platform-specific directory from **Settings → Troubleshooting → Open error log folder**. Portable builds use the same per-user log location regardless of where the EXE itself is stored.

For the durable security, privacy, and provider-authority rules, see [`docs/architecture/v2-architecture.md`](docs/architecture/v2-architecture.md).

---

## 🛠️ Development & Building

### Prerequisites

- [Node.js](https://nodejs.org/) `>= 22.15.0`
- [Rust](https://rustup.rs/) stable toolchain
- platform prerequisites required by [Tauri 2](https://v2.tauri.app/start/prerequisites/)

### Setup

```bash
git clone https://github.com/teeeeooo/token-lens.git
cd token-lens
npm ci
```

Run the core local checks:

```bash
npm run check
```

Launch the desktop app in development mode:

```bash
npm run dev
```

On Windows, build the unsigned NSIS installer and single-EXE portable artifacts under `dist-v2/`:

```bash
npm run package:windows
```

GitHub CI additionally runs Clippy with warnings denied, a native Tauri debug build, and `npm audit --audit-level=high` on both macOS and Windows.

---

## 🏛️ Architecture & Upstream Roots

Token Lens v2 uses **Tauri 2** for the desktop shell and pins **tokScale 4.15.1** as the current primary usage/quota dependency. Raw tokScale/provider payloads stay behind stable Rust domain contracts before reaching the renderer.

Token Lens-owned provider integrations remain deliberately narrow: they cover confirmed quota gaps plus the session metadata/detail behavior required by the retained UX. The current examples are Codex App Server fallback for missing base quota plus Business `individualLimit`; read-only Claude OAuth usage/spend enrichment with an interactive CLI `/usage` recovery fallback; read-only Gemini Code Assist quota using provider-owned access-token state with 401 re-read/one-shot retry and no CLI quota scraping; AGY quota; Codex/Claude/Gemini session metadata; and Codex/Claude per-turn usage detail.

Token Lens originated as a downstream of the MIT-licensed [Javis603/token-monitor](https://github.com/Javis603/token-monitor) project. The Electron-based v1 line initially tracked upstream Token Monitor **v0.53.0** at commit `0b17b1ec53ccd60508a645144ccb7db74027168c`.

v2 is an orphan lineage rather than a routine source-tree continuation of upstream. Maintenance follows these roles:

- **Token Monitor upstream** — semantic/reference source for relevant provider behavior, schema changes, and proven edge cases;
- **Token Lens v1** — historical implementation reference for validated UI behavior and security decisions;
- **tokScale** — primary evolving dependency for actual usage and supported subscription-quota behavior;
- **Token Lens-owned adapters** — reviewed, narrow integrations for confirmed gaps and retained session UX.

Relevant upstream changes are reviewed selectively rather than merged automatically into v2.

### Project documentation

- [`STATE.md`](STATE.md) — current implementation and validation state
- [`docs/README.md`](docs/README.md) — documentation map
- [`docs/architecture/v2-architecture.md`](docs/architecture/v2-architecture.md) — durable v2 architecture/security contract
- [`docs/migration/v1-porting-map.md`](docs/migration/v1-porting-map.md) — v1 → v2 preservation decisions

---

## 📄 License & Disclaimer

Distributed under the [MIT License](LICENSE).

Token Lens is an independent downstream project and is not affiliated with, sponsored by, or endorsed by the upstream Token Monitor maintainer, OpenAI, Anthropic, or Google.
