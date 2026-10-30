# Phase 4: Projects and Git

## Goal
Add repositories as projects, and a tested git service that all workspace features sit on.

## Depends on
Phase 3.

## In scope
- **Add project flows:** open a local repo, open a GitHub repo (via `gh repo list`, then clone into `~/circles/repos/`; see `docs/projects.md` for what the picker lists now), quick start (create new repo with an initial commit).
- **Validation:** is it a git repo, is it already added, is the path inside an existing worktree.
- **Base branch detection:** read `origin/HEAD`, fall back to `main`, then `master`. Stored per project, editable.
- **Git service** (shells out to the `git` CLI, never libgit2). Operations: `fetch`, `worktree add/remove/list/prune`, `branch` (create, rename, delete, exists), `status`, `diff` (numstat and patch), `log`, `rev-parse`, `merge-base`, `ahead/behind`.
- Structured error mapping for common git failures: branch already checked out, dirty tree, no remote, auth failure.
- Project removal (removes from Circles only, never deletes the repo).
- Project avatar (first letter or repo owner avatar from `gh`).

## Out of scope
Creating workspaces (phase 5), any UI beyond a minimal add-project dialog.

## Deliverables
- `apps/desktop/src-tauri/src/git/` with unit and integration tests against temp repos.
- Commands: `project_add_local`, `project_add_github`, `project_quick_start`, `project_list`, `project_remove`, `project_set_base_branch`.
- Minimal "Add project" dialog and a debug project list.

## Technical notes
- All git calls run with the resolved login env and `GIT_TERMINAL_PROMPT=0` so nothing hangs waiting for a credential.
- Use `-c core.quotepath=false` and parse porcelain v2 output.
- Every command has a timeout and returns a typed error.
- `gh` missing or unauthenticated is a normal state: the GitHub option shows a clear message with the fix command.
- Git author for repos created by quick start comes from the user's global config, never hard-coded.

## Done checklist
- [ ] Add local repo, GitHub repo, and quick start all work
- [ ] Base branch is detected correctly with and without `origin`
- [ ] Git service integration tests pass on temp repos
- [ ] Failure modes (no remote, dirty tree, branch checked out) return typed errors
- [ ] Removing a project leaves the repo untouched

## Manual test script
1. Add a local repo with an `origin`, confirm base is `main`.
2. Add a repo with no remote, confirm fallback.
3. Add from GitHub, confirm clone into `~/circles/repos/`.
4. Run `gh auth logout`, confirm the GitHub option explains how to fix it.
5. Remove the project, confirm the folder still exists.
