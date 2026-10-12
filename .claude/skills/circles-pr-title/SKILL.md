---
name: circles-pr-title
description: >-
  Write the title of a Circles pull request. Use when drafting or reviewing a PR title, when the user
  runs /circles-pr-title, or from circles-create-pr. The title is "KEY-n : <ticket summary>" with
  the Jira summary verbatim, because every PR has exactly one ticket. No emoji, no attribution.
---

# circles-pr-title

## Format

```
KEY-n : <ticket summary>
```

Example: `PROJ-45 : Add bot CRUD API`. A space before the colon.

- Read the ticket's current summary with the Jira tools (read-only) and use it exactly. If it changed
  since the branch was made, use the current one. Never reword it. If the wording is wrong, fix the
  ticket, with approval, not the title.
- One ticket per PR. Two tickets mean two PRs.
- No ticket means no PR title: file the ticket first (`circles-jira-tickets`).
- No emoji. No `WIP` or `[DRAFT]`: open a draft PR instead. No Claude or Anthropic attribution.

## Steps

1. Find the key: the branch name (`<type>/<KEY-n>-<slug>`), and check the commit subjects end with
   the same `(KEY-n)`. A mismatch means stop and ask.
2. Fetch the ticket's summary.
3. Print `KEY-n : <summary>`.
