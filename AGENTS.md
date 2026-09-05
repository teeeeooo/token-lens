# Repository task routing

Before implementation or architectural changes:

1. Read [`STATE.md`](STATE.md).
2. Read [`docs/README.md`](docs/README.md).
3. Read the relevant durable contract under `docs/`.

## v2 rules

- Treat v1 `main` as a reference implementation, not a merge target.
- Do not port v1 code merely because it exists.
- Prefer tokScale for usage, quota, model/session aggregation, and cost whenever tokScale already provides the required semantics.
- Keep Token Lens-owned provider code narrowly limited to confirmed gaps in tokScale.
- Preserve the established Token Lens UI/UX unless a task explicitly requires a change.
- Keep raw tokScale/provider payloads behind the v2 normalization boundary; renderer-facing contracts must remain stable.
- Codex, Claude, Gemini CLI, and Antigravity are the current runtime allowlist, not a permanent ceiling; add any future provider only through an explicit adapter plus authority, credential-boundary, privacy-contract, and regression-test review.
- Session metadata may expose provider-owned titles, timestamps, usage/tool metadata, and project/path basenames, but never prompt/response text, content previews, reasoning summaries, or title fallbacks derived from transcript content.
- Do not reintroduce removed providers, Hub/multi-device, broad account management, export, diagnostics/service status, or other v1 subsystems without an explicit architecture change.
- Record current status in `STATE.md`; use Git history for chronology rather than turning state documents into journals.
