# Phase 5: Workspaces

## Goal
Create, list, archive, and restore isolated worktree workspaces with a clear lifecycle.

## Depends on
Phase 4.

## In scope
- **Create:** fetch `origin/<base>`, create branch `<gh-username>/<random5>`, `git worktree add` at `~/circles/workspaces/<project>/<city>`.
- **City names:** curated list, unique per project. Fall back to `<city>-2` when exhausted.
- **Copy rules:** apply "Files to copy" and `.worktreeinclude` so gitignored files (`.env`) land in the new tree.
- **`.context/`:** create it, add it to the worktree's local exclude (never touches the repo's `.gitignore`).
- **Lifecycle state machine:** `creating`, `setup_running`, `ready`, `working`, `idle`, `blocked`, `archived`, `errored`. Transitions are validated in one place and emitted as events.
- **Base branch resolution:** per-workspace override, else project default. Offline falls back to the local base and records a warning event.
- **Archive:** run archive script (hook only, phase 7), warn if dirty and offer auto-commit, remove worktree, keep branch. **Restore:** recreate the worktree from the branch.
- **Rename branch**, **pin**, **sections** (user groups with order), **delete archived workspace record**.
- **Startup reconcile:** compare DB with `git worktree list`, mark missing trees as `errored`, prune dangling entries.
- **Event log lines** the chat shows later: "branched from origin/main", "created algiers and copied 540 files".

## Out of scope
Setup and run scripts (phase 7), agents (phase 8), UI polish (phase 6).

## Deliverables
- `apps/desktop/src-tauri/src/workspace/` with the state machine and service.
- Commands: `workspace_create`, `workspace_list`, `workspace_get`, `workspace_archive`, `workspace_restore`, `workspace_rename_branch`, `workspace_pin`, `section_*`.
- Tauri events: `workspace:state-changed`.
- Sidebar wired to real workspaces (replaces mock data).

## Technical notes
- One branch can only be checked out in one worktree. Surface git's error as "branch already in use by <workspace>".
- Never delete a branch automatically. Never delete a worktree with uncommitted changes without explicit confirmation.
- Creation is transactional: on failure, roll back the worktree and the DB row, and leave the workspace `errored` with a readable cause.
- Worktree paths contain only safe characters. Sanitise project names.

## Done checklist
- [ ] Create produces branch, worktree, and copied files
- [ ] Two workspaces can run side by side on the same project
- [ ] Archive and restore preserve the branch and work
- [ ] Dirty archive prompts and never loses changes
- [ ] Reconcile on startup fixes a manually deleted worktree
- [ ] State machine rejects invalid transitions (tested)
- [ ] Offline create falls back with a warning

## Manual test script
1. Create three workspaces in one project, confirm distinct cities and branches.
2. Edit a file in one, archive, confirm the dirty-tree prompt.
3. Restore it, confirm the edit is back.
4. Delete a worktree folder manually, relaunch, confirm it shows as errored.
5. Disconnect the network, create a workspace, confirm the warning line.
