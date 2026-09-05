# Token Lens

Token Lens v2 is a lightweight desktop dashboard for monitoring AI coding-tool usage and account quota.

Initial supported tools:

- Codex
- Claude
- Antigravity (AGY)

The v2 application uses Tauri 2 for the desktop shell and tokScale as the primary usage/quota data engine. It intentionally does not reproduce the broad provider framework of the original Token Monitor codebase. The current provider allowlist can be extended later through an explicit provider adapter and security/data-contract review.

## Product goals

- preserve the established Token Lens dashboard, limits view, charts, model/session exploration, tray behavior, and floating monitor UX;
- show current quota, short-window quota, weekly quota, reset time, and provider-specific quota details that are reliably available;
- show daily/weekly/monthly actual usage, model breakdown, session breakdown, token categories, and cost when tokScale can report it;
- remain substantially smaller and simpler than the Electron-based v1 application.

## Privacy boundary

Token Lens may parse local Codex/Claude session files to derive usage metadata, but renderer-facing session data must not contain prompt/response text. Provider-owned session titles (for example Codex thread titles or Claude `aiTitle`) are permitted for identifying a session; Token Lens must not create title fallbacks from raw prompts, responses, reasoning summaries, or transcript snippets.

See the durable [v2 architecture contract](docs/architecture/v2-architecture.md) for the authoritative security and privacy rules.

## Architecture

Read [`STATE.md`](STATE.md) first for the current engineering state, then [`docs/README.md`](docs/README.md) for documentation routing.

The durable v2 architecture contract is [`docs/architecture/v2-architecture.md`](docs/architecture/v2-architecture.md).

## Development

Prerequisites:

- Node.js
- Rust stable toolchain
- platform prerequisites required by Tauri 2

Install and check the skeleton:

```bash
npm install
npm run check
```

Run the desktop app during development:

```bash
npm run dev
```

## Lineage

Token Lens v2 lives on an orphan branch in the existing repository. The v1 `main` history remains a reference implementation and historical source for validated UI behavior and provider-specific edge cases; it is not a merge target for v2.
