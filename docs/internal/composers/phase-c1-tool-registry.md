# Phase C1: Tool registry and audit log

> **Built.** What it delivered, and what is still open, is in [status.md](status.md).

## Goal
One typed, permissioned list of Circles actions, usable without any model.

## Depends on
C0.

## In scope
- **Registry:** each tool is a Rust value with a name, a description, a JSON schema for its inputs, a tier, and an async `run`. Tools call the existing services (`workspace::service`, `project::service`, `harness::sessions`, `pr`, `review`). They never reach around them.
- **Caller context:** every call carries who is calling (a run id, or a person in a test) and which projects it may touch.
- **Policy:** a function decides allow, ask or deny from the Composer's setting for that tool (`allow`, `ask` or `off`), the project rules and the limits. Defaults ask for everything above `read`. The person may set any tool to `allow`.
- **Audit log:** a table of every call: time, caller, tool, redacted inputs, result summary, outcome, and the approval that allowed it.
- **Input checks:** schema validation first, then the same path and branch-name checks the rest of the app uses.
- **Result shaping:** results are short and structured. Large output is cut with a note that it was cut.
- **Dev surface:** a debug page that lists the tools and runs one by hand with a typed form, so each tool can be tried without a model.

## Out of scope
Models, MCP, the Composer itself, UI beyond the debug page.

## Deliverables
- `apps/desktop/src-tauri/src/tools/` (registry, policy, audit), `tools_*` migration, debug page.
- Commands: `tool_list`, `tool_call` (debug only, behind a dev flag).

## Technical notes
- A tool declares whether it is safe to repeat. Resume (C2) relies on this.
- Created things carry the call id so a repeated call can find what the first one made.
- Never log full inputs for tools that carry file contents. Log sizes and paths.

## Done checklist
- [ ] Every catalogue tool exists, with a schema and a tier
- [ ] A call outside the caller's allowed tools or projects is refused
- [ ] By default `write`, `external` and `destructive` tools ask, and a tool set to `allow` does not
- [ ] The audit log records allowed, denied and approved calls
- [ ] Bad inputs are refused before any service runs
- [ ] Tests cover each tool with fakes, and the policy table fully

## Manual test script
1. Open the debug page and list the tools.
2. Run `list_projects` and `create_workspace` by hand and check the audit log.
3. Try `delete_workspace` with the default setting and confirm it asks. Set it to `allow` and confirm it does not.
4. Try a call with a path outside the workspace and confirm it is refused.
