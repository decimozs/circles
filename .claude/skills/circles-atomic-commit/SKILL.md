---
name: circles-atomic-commit
description: >-
  Use for EVERY commit in the Circles repo, including an explicit "commit this". Commits are atomic
  conventional-commit units: one logical change per commit, independently revertible, with a
  why-not-what body, the ticket key in the subject, exact-path staging, the personal identity and
  signature, and no Claude or Anthropic attribution. Use when about to commit,
  when asked to "commit", "split into commits" or "make atomic commits", and after the checks go
  green. It never pushes to main and never merges.
---

# Circles atomic commits

Every commit goes through this skill. If you reach for `git commit` outside it, stop and come back.
One logical change per commit, each independently reviewable and revertible.

## Preconditions, in order

1. **A ticket, and not on `main`.** Every change has a ticket, even a small one, and its key is in
   the branch name (`<type>/<KEY-n>-<slug>`). `main` is protected: every change reaches it through a
   pull request. On `main`, or on a branch without a ticket key, stop: file the ticket with
   `circles-jira-tickets`, then create the branch with `circles-pr-branch`.
2. **Identity.** Run `git config user.name`, `git config user.email` and
   `git rev-parse --absolute-git-dir`. The repository lives under `~/Personal/`, so author and
   committer must be `decimozs <marlonadiguemartin548@gmail.com>`, the active `gh` account must be
   `decimozs` (never `Alfa-Marlon`), and commits must be SSH-signed with the personal key from
   `~/.gitconfig-personal` (never the work key). Flag a mismatch and stop.
3. **Checks are green for what changed.**
   - Frontend or packages touched: `pnpm lint`, `pnpm typecheck`, `pnpm check`, `pnpm test`.
   - `apps/desktop/src-tauri` or `packages/bindings` touched: also `cargo fmt --all --check`,
     `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
   - Docs and skills only (`docs/**`, `.claude/**`, `.github/**`, `*.md`): no code checks, but
     `pnpm lint` still runs because Biome covers JSON and YAML.
   Run them on the working tree before committing. A failing check is fixed first, not committed
   around. Quote the shortest decisive output when reporting.
4. **Stay current without breaking the stack.** Refresh `main` with
   `git fetch origin +refs/heads/main:refs/remotes/origin/main`. If this branch sits above another
   open PR's branch, do not `git rebase origin/main` (it would replay the lower commits). Run
   `circles-stack-rebase` instead.

## Rules

- **One logical change per commit.** A schema and its migration are one change; a schema and an
  unrelated rename are two.
- **One entity per commit.** One commit per module, component, command, skill (its `SKILL.md` plus
  its templates), config file group or document.
- **Each commit independently revertible.** `git revert <sha>` must leave a coherent state. A test
  lives in its unit's commit, or in the commit right after only when it needs the unit to compile.
- **Subject:** `<type>(<scope>): <imperative summary> (KEY-n)`, max 72 characters including the key,
  no trailing period. The key is the ticket of the branch.
  - Types: `feat`, `fix`, `refactor`, `perf`, `docs`, `test`, `chore`, `ci`, `build`, `style`, `revert`.
  - Scope is the area: `ui`, `bindings`, `config`, `desktop`, `composer`, `chat`, `workspace`, `git`,
    `pr`, `review`, `settings`, `terminal`, `integrations`, `smart`, `ci`, `docs`, `repo`.
  - Good: `fix(git): compare with the remote base, not the stale one (PROJ-12)`.
  - Bad: `update stuff`, `fix`, `WIP`, a good subject with no key.
- **Body is why, not what.** What was wrong or wanted, and how it fits the sequence. Skip the body
  only when the subject already carries the why. Wrap at about 72 characters.
- **No Claude or Anthropic attribution, ever.** No `Co-Authored-By:` trailer, no "Generated with"
  line, in a commit or a PR. A technical mention ("add the Anthropic provider") is fine.
  - Before committing scan the message: `grep -iE 'co-authored-by|generated with|claude code'`.
  - After committing check `git log -1 --format=%B`. Amend only to remove one, on an unpushed commit.
- **Stage exact paths.** `git add <those paths>`, never `git add -A` or `git add .`. Check
  `git diff --cached --stat` before committing. Do not fold unrelated changes in.
- **Never commit** secrets, `.env*`, `jira.env`, model files (`*.gguf`), databases or build output
  (`target/`, `dist/`).
- **Many small commits beat a few large ones.**

## Ordering

Dependencies before dependents:

1. config, tooling and docs that the change relies on
2. shared packages (`packages/config`, `packages/bindings`, `packages/ui`)
3. Rust core, then the frontend that uses it
4. tests that could not ship with their unit
5. follow-ups the change exposed

## Make the commit

```bash
git add <exact paths for this unit>
git diff --cached --stat
git commit -m "<type>(<scope>): <imperative summary>" -m "<why this change, how it fits>"
git log -1 --show-signature --format='author: %an <%ae>%ncommitter: %cn <%ce>%n%B'
```

The signature check can print a verification warning when no allowed-signers file is configured.
That is a local limitation, not a failure: confirm the commit has a `gpgsig` header with
`git cat-file -p HEAD | grep -c gpgsig`.

## After committing

- Pushing the branch is part of `circles-create-pr`. Never push to `main`; never merge.
- A history rewrite of pushed commits needs `--force-with-lease` and the owner's approval.
