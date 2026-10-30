# Phase 11: Checks, Todos, PRs

> Changed later: the todos were removed, in the app and in the backend (migration `0014_drop_todos`). Nothing about todos below applies any more. A merge is held back by checks, conflicts and requested changes. See `docs/pull-requests.md`.

## Goal
Merge readiness: Checks tab, merge-blocking todos, PR creation and updates via `gh`, and the "suggested next action" button.

## Depends on
Phases 9 and 10.

## In scope
- **Checks tab sections:** git status (ahead, behind, dirty, conflicts), PR metadata, CI and status checks, deployments, GitHub review threads, todos.
- **Todos (removed later, in the app and in the backend; open todos no longer block a merge):** manual add, edit, complete, delete. Agents can add via `@todos` context. Open todos block or warn on merge. Completed means done, irrelevant means delete.
- **PR creation** (Cmd+Shift+P): push branch, agent drafts title and body using the repo's PR instructions, create with `gh pr create`, store PR number and URL on the workspace.
- **Next action button** in the top bar, driven by workspace state: Create PR, Respond to review, Fix failing checks, Resolve conflicts, Merge, with the "Ready to merge" pill and PR chip (`#123 ↗`).
- **Fix checks and respond to review:** one click starts an agent turn with the failing logs or the unresolved threads as context.
- **Conflict resolution:** detect conflicts against base, one click asks the agent to rebase or merge with the conflict-resolution instructions.
- **Merge** via `gh pr merge` (squash, merge, rebase from settings), then offer archive.
- **Polling:** refresh PR and CI status on focus and on an interval while a PR is open.
- **Repository instruction presets** (general, review, PR, fix errors, conflicts, branch rename) read from project settings and injected into the matching actions.

## Out of scope
Linear, GitHub OAuth, non-GitHub hosts.

## Deliverables
- `apps/desktop/src-tauri/src/github/` (a `gh` wrapper with JSON output parsing), `apps/desktop/src/features/checks`, `apps/desktop/src/features/todos`, `apps/desktop/src/features/pr`.
- Commands: `pr_create`, `pr_status`, `pr_merge`, `checks_list`, `threads_list`, `thread_resolve`, `todo_*`.

## Technical notes
- All `gh` calls use `--json` and typed parsers. Handle unauthenticated, rate-limited, and offline states as normal UI states.
- Pushing uses the user's existing git credentials. Never store tokens.
- Merge is disabled with a clear reason when todos are open, checks fail, or conflicts exist, with an explicit override for the user.
- Branch auto-rename (phase 5 hook) runs here too: only before the first push.

## Done checklist
- [ ] Create PR from a workspace end to end
- [ ] Checks tab matches what GitHub shows
- [ ] Failing check starts a fix turn with logs attached
- [ ] Review threads show and resolve both ways
- [ ] Open todos block merge, override is explicit
- [ ] Merge then archive flow works
- [ ] Offline and unauthenticated states are handled

## Manual test script
1. In a test repo, make a change and press Cmd+Shift+P, confirm the PR on GitHub.
2. Add a failing CI check, press the fix action.
3. Leave a review comment on GitHub, confirm it appears and can be resolved.
4. Add a todo, try merge, confirm blocked, complete it, merge.
5. Turn off the network, confirm the Checks tab degrades gracefully.
