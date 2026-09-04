# Documentation Map

This directory contains the durable Token Lens v2 documentation. Keep it intentionally small.

## Reading order for a fresh session

1. [`../STATE.md`](../STATE.md) — current engineering state and next action.
2. [`architecture/v2-architecture.md`](architecture/v2-architecture.md) — durable v2 system contract.
3. [`migration/v1-porting-map.md`](migration/v1-porting-map.md) — read only when reusing or comparing v1 code.

## Authority

| Responsibility | Authority |
|---|---|
| Current implementation state / next action | `STATE.md` |
| Durable product and architecture decisions | `architecture/` |
| What may or may not be reused from v1 | `migration/` |
| Historical chronology and completed change history | Git history |

## Update rules

- Keep `STATE.md` current by replacing stale state rather than appending a diary.
- Change `architecture/` only when a durable system contract changes.
- Update `migration/` when a v1 source becomes a confirmed port/reference target or is explicitly rejected.
- Add new documentation categories only when repeated material needs a stable owner; do not pre-create a large taxonomy.
