# Jira conventions

Every change in this repository has a ticket, even a small one. This file is where the tickets live
and how they are named. The `circles-jira-tickets` skill reads and updates it.

## Project

- Site: **not set yet**
- Project key: **not set yet**
- Never use the employer Atlassian tenant (`team-anervea`). Circles is a personal project.

Until both are filled in, the only change that may be made without a ticket is the one that fills
them in (see `CLAUDE.md`, bootstrap).

## Naming

| Thing | Convention | Example |
|---|---|---|
| Ticket key | `KEY-<number>`, assigned by Jira | `PROJ-123` |
| Ticket summary | Imperative, verb first, at most 80 characters, no key, no trailing period | `Compare with the remote base in Create PR` |
| Epic summary | The area name only | `Composers` |
| Branch | `<type>/<KEY-n>-<slug>`, type in `feat fix refactor perf docs test chore ci build style revert` | `fix/PROJ-12-stale-base-in-create-pr` |
| Commit subject | `<type>(<scope>): <imperative summary> (KEY-n)`, at most 72 characters | `fix(git): compare with the remote base (PROJ-12)` |
| PR title | `KEY-n : <ticket summary>`, the Jira summary verbatim, a space before the colon | `PROJ-12 : Compare with the remote base in Create PR` |
| PR description | Starts with the ticket link | `https://<site>.atlassian.net/browse/PROJ-12` |

## Issue types

- **Epic:** one per area; no branch of its own.
- **Task:** one implementation unit; features, chores and small fixes. Has a parent Epic.
- **Bug:** a defect with steps to reproduce.
- **Spike:** time-boxed research with a written output, never production code.

## Labels

None defined yet. Add them here with the user's approval; do not invent labels in a ticket.

## Status

`To Do`, `In Progress` (work started), `In Review` (PR open), `Merged`, `Done` (verified on `main`).
Only a person moves a ticket to `Done`.
