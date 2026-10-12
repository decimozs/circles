---
name: circles-pr-title
description: >-
  Write the title of a Circles pull request. Use when drafting or reviewing a PR title, when the user
  runs /circles-pr-title, or from circles-create-pr. A PR with a ticket is titled
  "KEY-n : <ticket summary>" with the Jira summary verbatim; a PR without one is a conventional
  title "<type>(<scope>): <summary>". No emoji, no attribution.
---

# circles-pr-title

## Format

| Situation | Title | Example |
|---|---|---|
| The branch carries a ticket key | `KEY-n : <ticket summary>`, summary verbatim, a space before the colon | `PROJ-45 : Add bot CRUD API` |
| No ticket | `<type>(<scope>): <imperative summary>`, scope optional | `fix: stale base in Create PR` |

- **Ticket PR:** read the ticket's current summary with the Jira tools (read-only). Use it exactly.
  If it changed since the branch was made, use the current one. Never reword it.
- **No ticket:** the same style as the commit subjects, describing the whole PR, not one commit. For a
  PR that groups several fixes, name the theme or list at most two, then describe the rest in the
  body.
- Imperative mood, verb first, no trailing period, at most 80 characters.
- No emoji. No `WIP`, `[DRAFT]` or similar: use a draft PR instead. No Claude or Anthropic
  attribution.
- One ticket per PR. Two tickets mean two PRs.

## Steps

1. Find the key: the branch name (`<type>/<KEY-n>-<slug>`), then commit subjects ending `(KEY-n)`.
2. With a key, fetch the summary; without one, summarize `git log origin/main..HEAD --oneline` and
   `git diff --stat origin/main...HEAD` into one line.
3. Check the length and the rules above. Print the title.
