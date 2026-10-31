# Pull requests page: plan

A **Pull requests** item in the sidebar, below Composer, opens a page that lists the pull requests
of the repositories you work in. Opening one shows it in a right pane with Summary, Timeline and
Code, like the T3 Code screens this is modelled on. Status: built in phases 1 to 6. Phase 7 (Merge and Check out) was built and then removed again at the owner's request, so the pane only reads. Not built: the commit filter in the Code tab, read-only review threads in the diff, writing comments and reviews (phase 9). Open tabs live in the page, not in the route, so there is one route, `/pull-requests`. Phases 3 and 4 were built together, as were 5 and 6.

## What it is

- **Sidebar:** `Pull requests` (git-pull-request icon) under `Composer`, route `/pull-requests`.
  A small badge counts PRs that ask for your review.
- **List (full width when nothing is open):**
  - Search box that understands plain text, `label:bug`, `author:name`, `#123`.
  - `Sort` (updated, created, size), `Filters` (state, draft, checks, label, author, repository),
    `All` (the scope switch: this project or all projects), refresh button.
  - Groups: `Needs your review`, `Yours`, `Others`, each with a count.
  - Row: state icon, `#number`, title, checks icon, draft/review marker, author avatar and name,
    repository, `+additions -deletions`, relative time.
- **Right pane (list narrows to a column when a PR is open):** tabs of open PRs (`#177`, `#879`, `+`
  to go back to the list), maximise button, and per PR:
  - **Header:** `repo #n` with open-on-GitHub, title, author and "updated 8d ago", `gh pr checkout n`
    copy, base branch <- head branch, files changed and totals, `Merge` and `Check out` buttons, a
    `...` menu.
  - **Summary:** checks summary ("8 of 9 passing", expandable list), reviewers, labels, collapsible
    description rendered as Markdown, checks, comments with filter and newest-first toggle.
  - **Timeline:** "Pull request opened", commits (message, short sha, `+/-`), reviews and comments in
    one stream, newest first or oldest first.
  - **Code:** commit filter (`All commits`), `n files, 0 / n viewed`, unified or split, collapse all,
    per-file header with a `Viewed` checkbox.

## Decisions to make first

Each has a recommendation; the plan assumes these unless you say otherwise.

1. **Scope.** "Current workspaces or repository". Recommend: default to the repository of the
   project you are in, and `All` lists every project's repository. The list groups by repository
   when more than one shows.
2. **State.** Show open PRs by default; `Filters` adds merged and closed.
3. **Read-only first.** v1 reads, merges and checks out. Writing comments and reviews is a later
   phase. Reading is safe to ship alone and keeps the first PRs small.
4. **Code tab renderer.** Monaco's diff editor, as the Changes tab uses, mounted only for the
   files you open. The list of files comes from `gh api .../pulls/n/files` (status, rename source,
   additions, deletions, binary). The before and after text of an expanded file comes from
   `gh api .../contents/{path}?ref={sha}` (raw), or from the project's own git when both commits are
   already there (`git cat-file -e`; never fetch into the user's repository). Text is cached by
   `sha + path`, since a sha never changes. One or two files are expanded at a time; binary files
   and files over the size cap say "too large, open on GitHub".
5. **Check out.** Reuse the existing flow that creates a workspace from a PR branch (the create
   dialog already lists PRs). `Check out` opens that flow with the PR chosen; it does not change the
   current workspace.

## Data

All through `gh`, as the rest of Circles does, so accounts and the Environment setting apply
(`account_token_for`, `env_with_settings`). Nothing new is stored in the database; the list is cached
in memory by TanStack Query.

| Need | `gh` call |
|---|---|
| List | `gh pr list --state open --limit 50 --json number,title,author,url,updatedAt,createdAt,additions,deletions,changedFiles,isDraft,state,labels,reviewDecision,reviewRequests,statusCheckRollup,headRefName,baseRefName,headRepositoryOwner` per repository |
| Detail | `gh pr view n --json body,reviews,comments,commits,files,mergeable,mergeStateStatus,baseRefName,headRefName,reviewRequests,labels,statusCheckRollup,latestReviews` |
| Files | `gh api repos/{owner}/{repo}/pulls/n/files --paginate` (list, status, rename source, counts) |
| File text | `gh api repos/{owner}/{repo}/contents/{path}?ref={sha}` with the raw media type, base and head, for an expanded file only |
| Timeline events | `commits`, `reviews` and `comments` from `pr view`, merged and sorted in Rust |
| Merge | `gh pr merge n --merge\|--squash\|--rebase` (existing `MergeMethod`) |
| Review comments (line threads) | existing GraphQL in `github/pr.rs` generalised from workspace to repository and number |

Today `github/pr.rs` works from a workspace folder and the current branch. The new calls take a
**repository folder and a PR number**, so they work for PRs that no workspace is on.

## Backend (Rust, `apps/desktop/src-tauri`)

- New module `github/browse.rs`: serde structs and parsers (`parse_pr_rows`, `parse_pr_detail`,
  `parse_patch_files`, `build_timeline`) and `Gh` methods that take `(repo_dir, number)`.
- New `commands/pr_browse.rs`: `pr_browse_list(project_id | null)`, `pr_browse_detail(project_id,
  number)`, `pr_browse_files(project_id, number)`, `pr_browse_merge(project_id, number, method)`.
  Registered in `lib.rs`; the repository folder comes from the project's path.
- ts-rs types (`PrRow`, `PrDetail`, `TimelineEvent`, `PrFile`) exported to `packages/bindings` with
  `pnpm gen:types`.
- Fan-out over projects is bounded (4 at a time), each with the existing default timeout. One failing
  repository shows an inline error row and does not hide the others.
- Errors reuse `GhError` classification (not signed in, no access, rate limited).

## Frontend (`apps/desktop/src`)

- `features/prs/` (new): `api.ts` (queries), `pr-list.tsx`, `pr-row.tsx`, `pr-filters.tsx` (parse and
  apply the search syntax), `pr-detail.tsx`, `summary-tab.tsx`, `timeline-tab.tsx`, `code-tab.tsx`,
  `pr-tabs.tsx`, `store.ts` (open tabs, selected tab, sort, filters, viewed files; tabs and `Viewed`
  persisted in `localStorage`, wrapped in try/catch).
- Sidebar: one `NavLink` in `features/sidebar/sidebar.tsx` after Composer, same `NAV_ITEM` style.
- Route `/pull-requests` and `/pull-requests/:projectId/:number` in `src/routes`.
- Reuse: `features/chat/markdown.tsx` for descriptions and comments, the checks list from
  `features/pr/` where it fits, `@circles/ui` primitives, `IconButton`, the resizable layout used by
  the workbench.
- Refresh: on open, on the refresh button, and on window focus after 60 s. No timers while the page
  is hidden.
- Shortcuts: `g p` style entry in the command palette ("Go to pull requests") and an entry in
  `shortcuts-dialog.tsx` if a key is assigned.

## Security and limits

- Markdown from authors is untrusted: render with `react-markdown` and `remark-gfm` only, no raw
  HTML. Links open with `openUrl`; remote images follow the Content Security Policy, so check
  `img-src` before allowing PR images (avatars come from `avatars.githubusercontent.com`).
- Large PRs: a file over a size cap shows "too large, open on GitHub". Files start collapsed and at
  most two Monaco editors are mounted at once.
- Rate limits: one list call per repository per refresh; detail calls only on open.

## Phases (stacked PRs, each small and reviewable)

1. **Backend list.** `github/browse.rs` list types and parser, `pr_browse_list`, tests with fixture
   JSON and a fake `gh`. No UI.
2. **Sidebar item, route, list.** Nav link, empty and error states, rows, groups, search, sort,
   filters, refresh.
3. **Detail shell and Summary.** Right pane, tabs of open PRs, header, reviewers, labels,
   description, checks, comments (read-only).
4. **Timeline.** Detail `commits`, `reviews`, `comments` merged; order toggle.
5. **Code, file list.** Files endpoint, headers with counts, `Viewed` and commit filter, collapsed
   files, large and binary guards.
6. **Code, Monaco diff.** Lazy before and after text (local git first, then `gh`), the diff editor
   lifted to take two strings, unified and split, existing review threads shown read-only.
7. **Actions.** Merge (method choice, blockers shown like the existing merge flow), `Check out`
   through the create-workspace flow, copy `gh pr checkout`.
8. **Docs.** Update `docs/pull-requests.md`, `docs/shortcuts.md`, `CHANGELOG.md` with
   `circles-update-docs`. (Smaller doc edits go in the phase that causes them.)

Phase 9, later: write comments, submit reviews, request reviewers, edit labels, line comments in
Code.

## Tests

- Rust: parsers against recorded `gh` JSON (drafts, no reviewers, fork PRs, empty body, very long
  body), timeline ordering, a fake `gh` that fails for one repository, the account token path.
- Frontend (Vitest with the existing IPC mock): grouping and counts, search syntax, sort, tab
  open/close/restore, empty and error states, merge button disabled when blocked, `Viewed`
  persistence.
- Checks per PR: `pnpm lint`, `pnpm typecheck`, `pnpm check`, `pnpm test`, and for Rust
  `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`.
- Not testable here: a real run against a repository with many PRs and several accounts. That is
  checked by hand in the app before the last phase is merged.

## Process

Jira site and key are still unset, so each phase is a recorded "Ticket: None" exception, as with the
recent changes. Once the key is set, this becomes one Epic with a Task per phase.
