---
name: circles-create-pr
description: >-
  Push the current branch and open a Circles pull request, always stacked on the latest commits.
  Use when the user runs /circles-create-pr or asks to "create the PR", "open a PR" or "draft the
  PR". It first refreshes origin/main and makes sure this branch sits on the tip of the open PR
  stack (running circles-stack-rebase if not), then writes the title and body with
  circles-pr-title and circles-pr-description. Pushing the branch and opening the PR are
  authorized; it never merges, approves or enables auto-merge.
---

# circles-create-pr

`main` is protected: every change needs a pull request and the owner's approval. Pull requests here
are **stacked**: all open PRs form one linear chain on the latest `main`, every PR targets `main`, and
the owner merges the **tip once** with a merge commit, which lands the whole stack and closes the
PRs below it.

## Preconditions

1. **Branch.** Not `main`. The name matches `circles-pr-branch`. Otherwise stop.
2. **Identity.** `git config user.name`, `git config user.email`, `git rev-parse --absolute-git-dir`:
   `decimozs <marlonadiguemartin548@gmail.com>`, under `~/Personal/`. `gh auth status` must show
   `decimozs` active (never `Alfa-Marlon`). Commits SSH-signed with the personal key. Mismatch means
   stop.
3. **Clean tree.** Nothing uncommitted. Commits go through `circles-atomic-commit`; check
   `git log origin/main..HEAD` follows it (conventional subjects, no attribution). If not, fix with
   that skill first.
4. **Checks green on this HEAD.** The set from `circles-atomic-commit` step 3, for what changed. Keep
   the decisive output for the Evidence section. Do not open a PR over a failing check.

## Stack check (always, before drafting)

1. `git fetch origin +refs/heads/main:refs/remotes/origin/main` and
   `gh pr list --state open --json number,title,headRefName,baseRefName`.
2. The open PRs, plus this branch, must form one linear chain on the live `origin/main`:
   - the bottom branch is based on `origin/main`'s tip (`git merge-base` equals `origin/main`);
   - each branch contains the one below it (`git merge-base --is-ancestor <lower> <upper>`).
3. This branch must be built on the **top** of that chain. If `main` advanced, a PR below merged, or
   this branch is on an old base, **stop and run `circles-stack-rebase`**. Do not open the PR until
   the stack is current. "Always stacked on the latest commits" is not optional.
4. Record the position: `level n of m`, and the PRs below.

## Open it

1. Title from `circles-pr-title`, body from `circles-pr-description`, with the Stack section
   filled from the check above. Print both in full.
2. Push: `git push -u origin <branch>`. Never to `main`. Never `--force` without an explicit lease
   and the owner's approval.
3. Create: `gh pr create --base main --head <branch> --title "<title>" --body-file <file>` as
   `decimozs`. Write the body file in the scratchpad, not the repository. Add `--draft` when any
   acceptance criterion is `Fail`.
4. Read it back: `gh pr view <n> --json title,body,baseRefName,mergeable`. Check the base is `main`,
   the body has no template comments left, and no attribution.
5. Print the URL, the stack position, and one line on how to merge: merge the tip, with a merge
   commit.
6. CI: report `gh pr checks <n>` once if asked. Do not poll in a loop.

## Never

- Merge, approve, enable auto-merge, or push to `main`.
- Target another PR's branch as the base. Every PR in the stack targets `main`.
- Open a PR for a branch that is behind `origin/main` or not on the stack tip.
- Reuse another PR's body.
