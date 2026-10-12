# Circles

A local desktop app that runs Claude Code agents in parallel, each in its own git worktree, with
Composers that orchestrate them. Tauri 2 (Rust) and React 19, in a pnpm and Cargo monorepo.

Layout: `apps/desktop` (the app), `packages/ui`, `packages/bindings` (generated, never edited by
hand), `packages/config`. Docs: `docs/`, notably `docs/JIRA.md` and `docs/phases/`.

## Hard rules

1. **No change without a ticket, even a small one.** Before any change, check that a ticket exists
   (the branch name, then a Jira search). If none does, file it first with `circles-jira-tickets`:
   the full draft is shown and needs the user's approval. A one-line fix gets a one-paragraph
   ticket, not none. "Just do it" does not skip this.
2. **Stay in the ticket's scope.** Anything found outside it becomes a new ticket proposal, not an
   extra change.
3. **Never write to Jira (create, edit, comment, transition) without the user's approval in chat.**
4. **`main` is protected.** Never commit to it, push to it, merge a PR, approve a PR, or enable
   auto-merge. Every change is a pull request that the owner reviews and merges.
5. **Names carry the ticket.**
   - Branch: `<type>/<KEY-n>-<slug>`, from `circles-pr-branch`.
   - Commit subject: `<type>(<scope>): <imperative summary> (KEY-n)`, from `circles-atomic-commit`.
   - PR title: `KEY-n : <ticket summary>`, from `circles-pr-title`.
   - PR description starts with the ticket link, from `circles-pr-description`.
6. **Keep PRs stacked.** All open PRs form one linear chain on the latest `origin/main`. A new branch
   starts from the tip of the chain, every PR targets `main`, and the owner merges the tip once with
   a merge commit. `circles-create-pr` checks this and runs `circles-stack-rebase` when needed.
   Force-push only with an explicit lease and the owner's approval, never to `main`.
7. **Every commit goes through `circles-atomic-commit`.** One logical change and one entity per
   commit, exact-path staging (never `git add -A`), a why-not-what body.
8. **No Claude or Anthropic attribution** in a commit, a PR title or a PR body: no `Co-Authored-By`
   trailer, no "Generated with" line.
9. **Identity.** Before the first commit of a session run `git config user.name`,
   `git config user.email` and `git rev-parse --absolute-git-dir`. This repository is under
   `~/Personal/`: the identity is `decimozs <marlonadiguemartin548@gmail.com>`, the active `gh`
   account is `decimozs` (never `Alfa-Marlon`), and commits are SSH-signed with the personal key.
   Flag a mismatch instead of committing.
10. **Checks before a PR.** `pnpm lint`, `pnpm typecheck`, `pnpm check`, `pnpm test`; and when
    `apps/desktop/src-tauri` or `packages/bindings` changed, `cargo fmt --all --check`,
    `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`. Report what
    was run, with the result, and what was not run.

## Bootstrap

While `docs/JIRA.md` has no site and project key, a ticket cannot exist. Until they are set, the
only change allowed without a ticket is the one that records them, and anything the user explicitly
tells you is part of setting up this process (the skills, this file, `docs/JIRA.md`, the PR
template). Once they are set, this section no longer applies.

## Commands

```
pnpm install          pnpm tauri dev        pnpm lint        pnpm typecheck
pnpm check            pnpm test             pnpm gen:types   scripts/test-all.sh
cargo test --workspace   cargo clippy --workspace --all-targets -- -D warnings
```

Run commands from the repository root. Prefix shell commands with `rtk` (the user's global rule).

## Formatting

No emoji in tickets, PR titles or PR bodies. Acceptance-criteria Result values are the plain words
`Pending`, `Pass`, `Fail`.
