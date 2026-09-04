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
7. Run both downstream patchers. They deliberately fail if a security-sensitive upstream anchor changed unexpectedly:

```text
node scripts/downstream/apply-hardening.js
node scripts/downstream/apply-provider-hardening.js
```

8. Run `node --test tests-downstream/security-invariants.test.js`, `npm run lint`, and the production dependency audit.
9. Review the complete diff against the last Token Lens release before updating `main`.
10. Update the baseline metadata in this file only after the integration is accepted.

## Why the downstream patchers exist

High-churn upstream files such as `src/electron/main.js` and `src/shared/limitCollector.js` are kept structurally close to upstream. Rewriting them into a private architecture would make provider fixes expensive to absorb. Token Lens therefore applies small, assertion-based downstream patches. If upstream refactors a security-sensitive location, the patch stops rather than silently leaving a feature enabled.

## Provider/model evolution

Token Lens filters provider/tool IDs, not model IDs. New Claude/Codex/Antigravity model identifiers should therefore appear automatically while the relevant provider log/API schema remains compatible. Schema or quota API changes belong in the corresponding upstream provider implementation and should be selectively integrated.
