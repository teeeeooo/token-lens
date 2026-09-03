# Upstream Maintenance

Token Lens is a private downstream of:

- Upstream: `Javis603/token-monitor`
- Initial downstream baseline: upstream v0.53.0
- Exact upstream baseline commit: `0b17b1ec53ccd60508a645144ccb7db74027168c`
- Import branch: `import/upstream-v0.53.0`
- Import commit: `115a5e51e33e42736b7a220224f75936a5d59a1f`
- The import commit's direct parent is the exact upstream baseline commit.

The import commit removes only active upstream files under `.github/workflows/`. Token Lens owns its CI and release policy; upstream release workflows must never be imported blindly.

## Local Git remotes

Use:

```text
origin    https://github.com/teeeeooo/token-lens.git
upstream  https://github.com/Javis603/token-monitor.git
```

## Update procedure

1. Fetch upstream and record the exact candidate SHA/release.
2. Review upstream changes since the currently recorded baseline.
3. Pay particular attention to Codex, Claude Code, Antigravity, Electron security, credential, network, updater and tokscale changes.
4. Create a fresh integration branch from the current Token Lens `main`.
5. Merge or cherry-pick the upstream commits needed for the focused distribution. Prefer preserving upstream commit identity when a provider fix can be taken cleanly.
6. Never import upstream `.github/workflows/` as executable downstream CI without a separate review.
7. Run `node scripts/downstream/apply-hardening.js`. The patcher deliberately fails if a security-sensitive upstream anchor changed unexpectedly.
8. Run downstream invariant tests and lint.
9. Review the complete diff against the last Token Lens release before updating `main`.
10. Update the baseline metadata in this file only after the integration is accepted.

## Why the downstream patcher exists

`src/electron/main.js` is a high-churn upstream file. Rewriting it into a private architecture would make provider fixes expensive to absorb. Token Lens therefore keeps most upstream structure intact and applies a small, assertion-based downstream patch. If upstream refactors one of the security-sensitive locations, the patch stops rather than silently leaving a feature enabled.

## Provider/model evolution

Token Lens filters provider/tool IDs, not model IDs. New Claude/Codex/Antigravity model identifiers should therefore appear automatically while the relevant provider log/API schema remains compatible. Schema or quota API changes belong in the corresponding upstream provider implementation and should be selectively integrated.
