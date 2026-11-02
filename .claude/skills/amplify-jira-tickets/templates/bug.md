## Description
<1 to 3 sentences: what is broken and who it affects.>

## Steps to Reproduce
1. <precise step, with the exact input and where>
2. <step>
3. <step>

**Reproducibility:** <Always | Sometimes (n of m) | Once>

## Expected Behavior
<What should happen.>

## Actual Behavior
<What happens instead. Quote the exact error text.>

## Environment
- Severity: <Blocker | Major | Minor | Trivial>
- Found in: <branch, commit or version>
- Regression of: <KEY-n | Unknown | None>
- App commit: <git sha>
- OS: <macOS version>
- Claude Code version: <value | N/A>

## Evidence
<Logs (the shortest decisive lines), a screenshot or recording, the failing test name.>

## Suspected Cause
<Optional, labelled as a hypothesis. `None` if unknown.>

## Scope
- `<path>`: <where the fix is expected>

## Out of Scope
- <explicit exclusion>

## Acceptance Criteria

| # | Criterion (Given / When / Then) | Expected | Result | Evidence |
|---|---|---|---|---|
| AC1 | Given the steps to reproduce above, when they are repeated, then the defect no longer occurs | <expected behaviour> | Pending | |
| AC2 | Given <related case>, when <action>, then there is no regression | <result> | Pending | |

## Test Plan
```
<exact commands that must pass>
```
- Regression test: `<path>` fails before the fix and passes after.

## Definition of Done
- [ ] A failing regression test is added first
- [ ] All acceptance criteria are Pass with evidence
- [ ] The checks pass
- [ ] PR opened with this ticket linked
