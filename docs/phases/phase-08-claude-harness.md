# Phase 8: Claude Code harness

## Goal
A backend that drives Claude Code per session: spawn, stream, steer, interrupt, ask for permission, resume. No chat UI yet, a debug view is enough.

## Depends on
Phases 3, 5, 7.

## In scope
- **`Harness` trait** (`start`, `send`, `interrupt`, `stop`, `resume`) with `ClaudeCodeHarness` as the only implementation. Keeps room for other providers later.
- **Spawn:** `claude -p --input-format stream-json --output-format stream-json --verbose --include-partial-messages --permission-mode manual --permission-prompt-tool stdio`, cwd set to the worktree, login env applied.
- **Event parser:** NDJSON lines into typed events (`system:init`, `stream_event` deltas, `assistant`, `user/tool_result`, `result`, `control_request`, `rate_limit_event`). Unknown types are stored raw and ignored by the UI.
- **Persistence:** every raw event into `events`, normalised rows into `messages` (text, thinking, tool call, tool result). Raw JSON is the source of truth.
- **Session model:** many sessions (chat tabs) per workspace, each with its own process and Claude `session_id`.
- **Controls:** send message, steer mid-turn, interrupt (`control_request` `interrupt`), stop, resume (`--resume <id>`), fork (`--fork-session`).
- **Permissions:** `can_use_tool` requests become a `blocked` workspace state plus a UI request. Replies: allow once, allow always for the session, deny. Modes: manual, accept edits, plan, bypass (with a clear warning).
- **Model and effort** options passed through to the CLI.
- **Lifecycle:** idle to working to idle, with `blocked` during permission prompts. Rate limit info surfaced.
- **Crash handling:** process exit mid-turn marks the session `interrupted`, never loses stored events. On startup, mark stale `working` sessions as interrupted.
- **Notification hook:** emit "agent finished" events for phase 13 notifications.

## Out of scope
Chat rendering (phase 9), checkpoints (phase 12), other harnesses.

## Deliverables
- `apps/desktop/src-tauri/src/harness/{mod,claude,parser,permissions}.rs`
- Commands: `session_create`, `session_send`, `session_interrupt`, `session_stop`, `session_resume`, `permission_reply`.
- Stream of session events to the frontend over a `Channel`.
- Developer debug panel (hidden route) showing the raw stream.
- Replay tests using the spike's captured NDJSON files as fixtures.

## Technical notes
- Spike facts: one persistent process handles many turns, interrupt acks with `control_response`, a mid-turn user message is injected, forking is whole-session only.
- Backpressure: parse on a dedicated task, forward batched deltas, never block the reader.
- Always close stdin and terminate the process group on stop.
- Resolve `claude` via the login env. Show a clear error if missing, with an install hint.
- Pass `--session-id` when creating, so we know the id before the first event.

## Done checklist
- [ ] Send a prompt, receive streamed text and tool events in the debug panel
- [ ] Steer and interrupt work during a long turn
- [ ] A permission request blocks, then continues after allow or deny
- [ ] Resume reopens a session with context intact
- [ ] Killing the process mid-turn marks the session interrupted, events preserved
- [ ] Parser fixtures pass, unknown events are tolerated
- [ ] Two sessions in two workspaces run in parallel without crosstalk

## Manual test script
1. Create a session in a workspace, ask Claude to read a file and edit another.
2. Watch the raw stream, confirm events are in the DB.
3. During a long reply, send a steering message, then interrupt.
4. Trigger a permission prompt, deny once, allow once.
5. `kill` the claude process, confirm the interrupted state.
6. Quit and relaunch, resume the session, ask about the earlier conversation.
