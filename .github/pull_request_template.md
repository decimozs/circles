## Description
<!-- First line: the ticket link, when there is a ticket. Then 1 to 3 sentences: what was wrong or wanted, and why. -->
**Ticket:** <https://your-site.atlassian.net/browse/KEY-n, or "None">

## Summary
- <what a reviewer should know in ten seconds>
- <second point>

## Changes
<!-- Group by area. Must match `git diff --stat origin/main...HEAD`. -->
- `apps/desktop/src-tauri/...`: <module and what changed>
- `apps/desktop/src/...`: <module and what changed>
- `packages/...`: <what changed>

**Outside the intent of this PR (justify or remove):** None

## Evidence
<!-- Exact commands and shortest decisive results. Screenshots or recordings for UI changes. -->
```
pnpm lint        # <result>
pnpm typecheck   # <result>
pnpm test        # <n passed>
cargo test --workspace   # <n passed>
```

## Acceptance Criteria
<!-- Only with a ticket: copy its table and fill Result and Evidence. Otherwise delete this section. -->

| # | Criterion (Given / When / Then) | Expected | Result | Evidence |
|---|---|---|---|---|
| AC1 | Given ..., when ..., then ... | <expected> | Pending / Pass / Fail | <test name or output line> |

## Stack
<!-- Written by circles-create-pr. -->
- Position: <level n of m>
- Below: <#n title, or "none: on main">
- Merge: the tip of the stack, once, with a merge commit.
