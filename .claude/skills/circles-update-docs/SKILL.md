---
name: circles-update-docs
description: >-
  Make sure the Circles docs match a change. Use before committing or opening a PR, when the user
  runs /circles-update-docs, or asks to "update the docs", "check the docs" or "does this need docs".
  It maps the diff to the user guide pages, the CHANGELOG, the troubleshooting page and the
  internal docs, edits what is stale, and reports what it checked and what it left alone.
---

# circles-update-docs

`docs/` is the user guide: it describes what the app does today, for a person using it. A change that
alters what a user sees, can do, or must set up is not finished until the guide says so.

## 1. Read the change

```bash
git diff --stat origin/main...HEAD
git diff origin/main...HEAD
```

Include uncommitted work (`git status --short`). Read the diff, not only the file names.

## 2. Does the user notice?

Docs are needed when the change does any of these:

- adds, removes, renames or moves a screen, tab, button, menu item, setting or shortcut
- changes a default, a limit, a message the user reads, or when something happens
- changes what Circles sends to Claude, GitHub, Jira or Anthropic, or what it stores
- adds a new way something can fail, or removes one (troubleshooting)
- changes setup, install, build or release steps

No docs are needed for a pure refactor, a test, a CI tweak or a fix that restores what the guide
already says. Say so in the report and name why. Still check that nothing the guide says became false.

## 3. Map the change to pages

| Code area | Page |
|---|---|
| First run, setup checklist, install | `docs/getting-started.md` |
| Projects, GitHub picker, learned conventions | `docs/projects.md` |
| Workspaces, files, changes, graph, terminal, scripts | `docs/workspaces.md` |
| Chats, permissions, checkpoints, side questions, titles | `docs/chats.md` |
| Pull requests, checks, drafting, merging | `docs/pull-requests.md` |
| Composers, cards, triggers | `docs/composers.md` |
| Jira integration | `docs/jira.md` |
| Settings pages, `settings.toml`, environment, accounts | `docs/settings.md` |
| Shortcuts, sidebar order, command palette | `docs/shortcuts.md` |
| An error the user can meet and how to get out of it | `docs/troubleshooting.md` |
| Plans, audits, design decisions | `docs/internal/` |
| Jira and ticket conventions | `.github/JIRA.md` |
| Rules for agents and contributors | `CLAUDE.md`, `CONTRIBUTING.md` |

A new page is added to the reading order in `docs/README.md`. A removed page leaves no dead links:
`rtk grep -rn "<old-name>" docs README.md CLAUDE.md CONTRIBUTING.md .claude .github`.

## 4. Edit

- Describe the app as it is now. No history, no "now", no "previously", no PR or ticket numbers.
- Write for a user: what they see and do, not module or function names. Names of settings, buttons
  and menu items must match the UI text exactly; check the strings in `apps/desktop/src`.
- Keep the page's existing tone, heading style and length. Edit the sentence that went stale; do not
  append a paragraph.
- Delete text for anything removed. Search for the removed name across `docs/` and the root files.
- Add a `CHANGELOG.md` entry under `## Unreleased` in `### Added`, `### Changed` or `### Fixed`,
  one line, in the user's words. Skip it only for changes with no visible effect.
- Plain words and no emoji, as in PRs.

## 5. Verify

- Every `[text](path.md)` link in the touched pages resolves: `rtk grep -rno "](\([^)#]*\.md\)" docs`
  and check the targets exist.
- Every setting key, command, shortcut and menu label you wrote exists in the code. Open the file.
- Anything you could not confirm is written down as unconfirmed in the report; it is not guessed.

## 6. Report

End with three lines:

- **Updated:** the doc files changed, and the one-line reason for each.
- **Checked, no change needed:** pages read that already match.
- **Not checked:** what you could not verify (for example behavior you did not see running).

The docs change goes in the same commit as the code that caused it when it is small, or in its own
`docs(...)` commit through `circles-atomic-commit` when it is large. The PR description's Changes
list names the docs files.
