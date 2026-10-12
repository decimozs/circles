## Description
<2 to 4 sentences: what will exist after this ticket that does not exist now, and why it matters.>

## Context
<Background an engineer or agent needs: related tickets, modules involved, constraints.>

## Scope
<Exact areas and files expected to change.>
- `apps/desktop/src-tauri/src/<module>.rs`: <what changes>
- `apps/desktop/src/features/<feature>/<file>.tsx`: <what changes>

## Out of Scope
- <explicit exclusion>

## Acceptance Criteria

| # | Criterion (Given / When / Then) | Expected | Result | Evidence |
|---|---|---|---|---|
| AC1 | Given <state>, when <action>, then <outcome> | <observable result> | Pending | |
| AC2 | Given <state>, when <action>, then <outcome> | <observable result> | Pending | |

## Test Plan
```
<exact commands that must pass>
```
- Test files: `<path>`

## Dependencies
- Blocked by: <KEY-n | None>
- Blocks: <KEY-n | None>

## Definition of Done
- [ ] All acceptance criteria are Pass with evidence
- [ ] `pnpm lint`, `pnpm typecheck`, `pnpm test` pass; `cargo test --workspace` when Rust changed
- [ ] Tests added for new behaviour
- [ ] Docs updated if behaviour or structure changed
- [ ] PR opened with this ticket linked
