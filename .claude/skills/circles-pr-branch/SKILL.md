---
name: circles-pr-branch
description: >-
  Create a correctly named git branch for a Circles ticket, always on top of the open PR stack. Use
  when the user runs /circles-pr-branch, or asks to "create a branch", "start a branch for ..." or
  "branch for KEY-123". A ticket is required: names are <type>/<KEY-n>-<slug>. Verifies identity,
  builds from the stack tip, never tracks main, never pushes.
---

# circles-pr-branch

## Name

```
<type>/<KEY-n>-<slug>        KEY-n is the ticket, like PROJ-123
```

**Every change has a ticket, even a small one.** No ticket means no branch: file it first with
`circles-jira-tickets` (draft, then the user's approval), then come back. The one exception is
the bootstrap in `CLAUDE.md`, while `docs/JIRA.md` has no project key yet.

- `type`: `feat`, `fix`, `refactor`, `perf`, `docs`, `test`, `chore`, `ci`, `build`, `style`, `revert`.
- `slug`: lowercase words joined by dashes, 2 to 6 words, at most 40 characters, filler words removed
  (a, an, the, to, for, of, and). Letters, digits and dashes only; no leading, trailing or double
  dashes.
- Regex: `^(feat|fix|refactor|perf|docs|test|chore|ci|build|style|revert)/[A-Z][A-Z0-9]+-[0-9]+(-[a-z0-9]+){1,5}$`
- This is the rule Circles can give Composers: the project's `branch_pattern` setting
  `{type}/{ticket}-{slug}`.

Examples: `fix/PROJ-12-stale-base-in-create-pr`, `ci/PROJ-31-shard-frontend-tests`,
`feat/PROJ-45-add-bot-crud-api`.

## Type

Pick from what the change is. When there is a ticket, default from its Jira type (Bug is `fix`,
Task is `feat`, Spike is `chore`) and let the user override. If unclear, ask.

## Steps

1. **Ticket.** Get the ticket key and read it (read-only): summary, type, acceptance criteria. If
   there is none, stop and file it with `circles-jira-tickets`. Do not invent a key. If the ticket
   is an Epic, stop: epics have no branch.
2. **Identity.** `git config user.name`, `git config user.email`, `git rev-parse --absolute-git-dir`:
   `decimozs <marlonadiguemartin548@gmail.com>`, repository under `~/Personal/`, active `gh` account
   `decimozs`. Flag a mismatch and stop.
3. **Clean tree.** `git status --short` must be empty, or the changes must be intended for the new
   branch (say so). Otherwise stop and report.
4. **Refresh.** `git fetch origin +refs/heads/main:refs/remotes/origin/main`.
5. **Choose the base so PRs stay stacked.** List open PRs:
   `gh pr list --state open --json number,headRefName,baseRefName`.
   - None: base on `origin/main`.
   - Some: they form one linear chain. Base the new branch on the **tip** of that chain (the
     branch no other open PR is built on). First confirm the chain is current with `main`
     (`git merge-base --is-ancestor origin/main origin/<tip>`); if not, run `circles-stack-rebase`
     before branching.
   - Show the proposed base and ask the user to confirm. A branch that is independent of the stack
     only when the user says so explicitly.
6. **Build and validate** the name against the regex above. Print it with how it was derived.
7. **Check it is free.** `git branch --list <name>` and `git ls-remote --heads origin <name>`.
8. **Create.** `git switch -c <name> --no-track <base>`. Always `--no-track`: without it the branch
   would track its base and a bare `git push` could target `main`. Use
   `git worktree add ../circles-<slug> -b <name> <base>` only when asked for a worktree.
9. Print the base, the stack position and the ticket's acceptance criteria, so the work targets them.

## Never

- Push, or comment on Jira. Both need their own approval.
- Branch from a stale `main` or a stale stack tip.
- Commit on `main`: it is protected.
