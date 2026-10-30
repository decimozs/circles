# Phase 12: Checkpoints

## Goal
Turn-by-turn snapshots with safe revert of both code and conversation.

## Depends on
Phases 8 and 9.

## In scope
- **Snapshot before every user turn:** capture the working tree (tracked, modified, and untracked, not ignored) as a tree object, write a commit, and point `refs/circles/checkpoints/<workspace>/<turn>` at it. No effect on branch history or the index.
- **Checkpoint record:** DB row linking turn, ref, Claude `session_id`, and message position.
- **Revert UI:** hover a message, click revert, confirm dialog stating that this permanently deletes that turn and all later turns. Shows a preview (files that will change).
- **Revert mechanics:** restore files from the checkpoint ref, truncate messages and events from that turn onward, and re-establish a Claude session that matches (resume the last good session if reverting at the tail, otherwise start a new session and replay the retained history as context).
- **Safety:** auto-snapshot the current state before reverting ("pre-revert" ref) so a revert can itself be undone for a limited time.
- **Concurrent chats:** warn when other sessions in the same workspace are active, and block revert while a turn is running.
- **Cleanup:** prune checkpoint refs on archive or after N days, configurable.
- **Fork:** "branch conversation from here" creates a new tab using `--resume --fork-session` where the turn is last, or replay otherwise.

## Out of scope
Cross-workspace restore, cloud backup.

## Deliverables
- `apps/desktop/src-tauri/src/checkpoint/`, revert action in the chat UI, pre-revert undo toast.
- Commands: `checkpoint_list`, `checkpoint_preview`, `checkpoint_revert`, `checkpoint_undo_revert`.

## Technical notes
- Use a temporary index (`GIT_INDEX_FILE`) and `git write-tree` so the user's index is never touched.
- Spike fact: `--fork-session` forks the whole session only, so mid-history revert relies on our own message log and replay.
- Large untracked files are skipped past a size cap, with a warning shown at revert time.
- Revert is atomic: restore into a staging step, only swap when it all succeeds.

## Done checklist
- [ ] Every turn creates a checkpoint, and branch history is untouched
- [ ] Revert restores files exactly, including deleted and untracked files
- [ ] The agent has no memory of reverted turns afterwards
- [ ] Undo of a revert works
- [ ] Revert is blocked while a turn is running
- [ ] Failed revert leaves the workspace unchanged
- [ ] Refs are pruned on archive

## Manual test script
1. Run three turns that each change different files.
2. Revert to turn 2, confirm the files and the chat match.
3. Ask the agent about turn 3, confirm it has no knowledge of it.
4. Undo the revert, confirm turn 3 returns.
5. Try reverting during a running turn, confirm it is blocked.
6. `git log` and `git status`, confirm nothing changed in the branch.
