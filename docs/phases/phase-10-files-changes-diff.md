# Phase 10: Files, Changes, Monaco diff

## Goal
The right panel: browse files, see changes, review diffs in Monaco, and comment on lines for the agent.

## Depends on
Phase 5. Works best after phase 9.

## In scope
- **Right panel tabs:** All files, Changes (with count), Checks (stub until phase 11), Review.
- **All files:** lazy file tree with icons, search, show or hide ignored files, open in Monaco (read-only by default, optional edit), context menu (copy path, reveal in Finder, open in editor, attach to chat).
- **Changes:** list of changed files with `+/-` counts and status icons, grouped optionally by directory, filter by commit or uncommitted. Live updates from a file watcher.
- **Diff viewer** (Cmd+Shift+D): Monaco diff editor, unified and split toggle, file navigation (next and previous), language detection, large-file guard, binary and image file handling.
- **Line comments:** click the gutter to comment on a line or range. Comments are sent to the agent as structured context ("fix this" with file and lines). Stored locally, resolvable.
- **Top-bar actions:** open in Cursor or VS Code (detected apps), copy path.
- **Review tab:** "Run review" starts an agent session with a review prompt (and the repo's review instructions) over the diff. Findings appear as comments.

## Out of scope
GitHub review comments sync and PR status (phase 11).

## Deliverables
- `apps/desktop/src/features/files`, `apps/desktop/src/features/changes`, `apps/desktop/src/features/diff`, Monaco setup with theme tokens mapped to light and dark.
- Backend: `fs_list`, `fs_read`, `diff_files`, `diff_patch`, file watcher using `notify` with debounce.

## Technical notes
- Load Monaco workers locally (Tauri has no CDN access guarantee). Keep bundle lazy-loaded.
- Watcher ignores `.git`, `node_modules`, and build output. Debounce about 200ms, coalesce events.
- Diff base: merge-base with the workspace base branch for the "all changes" view.
- Never let file reads escape the worktree (canonicalise and check the prefix).

## Done checklist
- [ ] Tree handles large repos without freezing
- [ ] Changes update live while the agent edits
- [ ] Monaco diff themes match light and dark
- [ ] Line comments reach the agent and can be resolved
- [ ] Binary and huge files degrade gracefully
- [ ] Path traversal is blocked (tested)
- [ ] Open in editor works for installed apps

## Manual test script
1. Ask the agent to edit three files, watch Changes update live.
2. Open the diff, toggle unified and split, navigate between files.
3. Comment on a line, send to the agent, then resolve it.
4. Open a binary file and a very large file.
5. Try opening `../../etc/hosts` through a crafted path, confirm it is blocked.
