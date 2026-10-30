# Phase 9: Chat UI

## Goal
The full workspace center column: tabs, message list, tool calls, composer, permission prompts.

## Depends on
Phases 6 and 8.

## In scope
- **Chat tabs:** create, rename, close, reorder. A history popover to reopen closed sessions.
- **Empty state:** event lines ("You're in a new copy of ... called algiers", "Branched ... from origin/main", "Created ... and copied N files", "Optional: add a setup script") and suggestion chips.
- **Message list:**
  - User bubbles with attachments and file chips.
  - Streaming markdown for assistant text (code blocks with copy, tables, links).
  - Collapsible tool-call groups ("N tool calls") with per-tool renderers: Read, Edit/Write (inline mini diff), Bash (command and output), Grep/Glob, Task, others generic.
  - Thinking blocks (collapsed).
  - Turn summary row: duration, copy, menu, changed-file chips with `+/-`.
  - Scroll-to-bottom button, sticky autoscroll that pauses when the user scrolls up, and a right-edge turn minimap.
- **Composer:**
  - `@` file mentions (search the worktree), `@todos` (removed later with the todos), `/` slash commands (project and user `.claude/commands`, plus built-ins such as `/add-dir`), image and file attachments, paste handling (auto-convert text over 5000 chars into an attachment).
  - Model and effort picker, Plan mode toggle, Fast mode toggle.
  - Send with Enter or Cmd+Enter (setting). While working, the button becomes Stop. Follow-up behavior: Steer or Queue (setting), Cmd+Enter queues.
  - Queued message chips, editable and cancellable.
- **Permission prompts:** inline card (tool, input preview, Allow, Allow for session, Deny) that matches the `blocked` state. Notification badge in the sidebar.
- **Sidebar status:** spinner when working, dot when blocked, bold when unread.
- First prompt from the Create modal is sent automatically on first open.

## Out of scope
Right panel (phase 10), checkpoints revert UI (phase 12), notifications settings (phase 13).

## Deliverables
- `apps/desktop/src/features/chat/*`, tool-renderer registry, composer, permission card.

## Technical notes
- Virtualise the message list (long sessions). Keep streaming smooth: update only the active message component.
- Render from the normalised `messages` rows, fall back to raw event view for unknown block types.
- Attachments are copied into `<worktree>/.context/attachments/`, never committed.
- Slash command discovery reads `.claude/commands` and skills directories.

## Done checklist
- [ ] Streaming text is smooth with long responses
- [ ] Every common tool call renders readably, unknown ones render generically
- [ ] Steer, queue, interrupt, and stop work from the composer
- [ ] `@file`, `@todos`, slash commands, and attachments work
- [ ] Permission card blocks and resumes the turn
- [ ] Long session (1000+ messages) scrolls without lag
- [ ] Reopening a workspace restores all tabs and history

## Manual test script
1. Create a workspace from the Create modal, confirm the first prompt auto-sends.
2. Ask for a multi-file change, expand the tool calls, check the Edit renderers.
3. During a turn, send a steering message, then a queued one.
4. `@mention` a file and attach a screenshot.
5. Trigger a permission prompt and answer it.
6. Quit, relaunch, confirm tabs and history are intact.
