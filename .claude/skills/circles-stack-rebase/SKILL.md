---
name: circles-stack-rebase
description: >-
  Rebase the open Circles PR branches into one linear stack on the latest origin/main. Use when the
  user asks to "stack these PRs", "restack", "rebase the stack onto main", after main moved or a PR
  merged, and before opening a PR when other PRs are open (circles-create-pr calls it). Always
  refreshes origin/main first. Every PR's base stays main. Plans first; force-pushes and PR base
  edits only after explicit approval.
---

# circles-stack-rebase

Make the open PR branches one clean chain on the latest `main`, then push each with a lease. `main` is
protected and is never pushed to. The owner merges the **tip once** with a merge commit, so the PRs
below it close by themselves and the branches above a merged PR need no restack.

## Hard rules

- **Plan, then ask.** Rebase locally in an isolated worktree. Before any push or PR base edit, print
  the plan (order, old and new SHAs, conflicts resolved, check results) and get an explicit yes.
- **Identity.** `git config user.name`, `git config user.email`, `git rev-parse --absolute-git-dir`:
  `decimozs <marlonadiguemartin548@gmail.com>`, signed with the personal key, `gh` as `decimozs`.
  A rebase rewrites the committer of every replayed commit, so a wrong identity corrupts the stack.
- **Force-push only with an explicit lease**: `--force-with-lease=<branch>:<live sha>`. Never bare
  `--force`. Never to `main`.
- **Always start by refreshing `main`**, every run: it is force-refreshed with a leading `+`.
- **Isolate.** Work in `/tmp/circles-stack` (a detached worktree), not the user's checkout. Stop on the
  first collision signal: the HEAD sha changed under you, or a remote tip differs from the plan.
- **Checks per level** before each push, for what changed (see `circles-atomic-commit`).
- **Never** merge, enable auto-merge, approve, or change Jira from here.

## 1. Preflight

```bash
git fetch origin +refs/heads/main:refs/remotes/origin/main
MAIN=$(git rev-parse origin/main)
gh pr list --state open --json number,title,headRefName,baseRefName
# the bottom of the stack is the lowest open branch; the others are above it
git fetch origin "+refs/heads/$BOTTOM:refs/remotes/origin/$BOTTOM"
BASE=$(git merge-base "origin/$BOTTOM" origin/main)
[ "$BASE" = "$MAIN" ] && echo "STACK IS CURRENT" || echo "MAIN ADVANCED, REBASE REQUIRED"
```

If current and each branch already contains the one below it, say so and stop.

## 2. Order

Dependency order first, then the order the PRs were opened (ascending number). Show it and confirm if
it is not simply ascending.

## 3. Restack

```bash
git worktree add --detach /tmp/circles-stack "$MAIN" && cd /tmp/circles-stack
# save the old remote tips before touching anything
for B in "${BRANCHES[@]}"; do echo "$B $(git rev-parse origin/$B)"; done
```

For each branch bottom to tip:

```bash
git rebase --onto <new_parent_tip> <old_parent_tip> <branch>
```

- First branch: `old_parent_tip = git merge-base origin/<branch> origin/main`, `new_parent_tip = origin/main`.
- Later branches: `old_parent_tip` is the saved old tip of the branch below, `new_parent_tip` its new
  tip after the rebase.
- Resolve conflicts to the branch's intended design and list each in the plan.
- Run the checks at each level.

A squash or rebase merge would break patch identity. Merging is done with a **merge commit**, so
`git cherry` and patch matching are not needed. If a PR was merged another way anyway, restack each
branch above with `--onto origin/main <old-parent-tip>`.

## 4. Plan checkpoint, before any push

| Level | Branch | Old tip | New tip | Conflicts | Checks |
|---|---|---|---|---|---|

State the new base (`origin/main` sha) and wait for a clear yes.

## 5. Push, bottom to tip

```bash
R=$(git ls-remote origin "refs/heads/$B" | awk '{print $1}')
git push origin "$B:$B" --force-with-lease="$B:$R"
git update-ref "refs/remotes/origin/$B" "$(git rev-parse $B)"
```

Re-check the live remote tip right before each push and abort if it moved.

## 6. PR bases and cleanup

Every PR targets `main`:

```bash
gh api -X PATCH repos/decimozs/circles/pulls/<N> -f base=main
```

Then `git worktree remove /tmp/circles-stack --force && git worktree prune`.

## Topology helper

```bash
for B in "${BRANCHES[@]}"; do
  echo "$B ahead=$(git rev-list --count origin/main..origin/$B) fork=$(git merge-base origin/$B origin/main)"
done
git merge-base --is-ancestor origin/<lower> origin/<upper> && echo LINEAR || echo NOT-LINEAR
```
