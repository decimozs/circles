---
name: circles-jira-tickets
description: >-
  Write Jira tickets (Epic, Task, Bug, Spike) for Circles in markdown from templates, with a
  description, scope, steps to reproduce for bugs, a checkable acceptance-criteria table, a test
  plan and dependencies. Use when the user runs /circles-jira-tickets, or asks to "create a
  ticket", "file a bug", "write the tickets", or "break this down". It always shows the full draft
  and waits for explicit approval before anything is written to Jira.
---

# circles-jira-tickets

## Before the first ticket

Circles has no Jira project recorded yet. Ask the user once for the **site** and the **project key**,
and write both into `docs/JIRA.md` (create it if missing) so they are not asked again. Never guess a
key, and never use the employer tenant (`team-anervea`): Circles is a personal project.

Transport, in order of preference:
1. A helper `scripts/jira/jira.mjs` with credentials in the gitignored `jira.env`, when it exists.
   It does not exist in this repository yet.
2. The connected Atlassian tools (read: search, get issue; write: create issue). Check that the
   connected site is the one in `docs/JIRA.md` before the first write.

Credentials are never printed, logged, committed, or pasted into a ticket.

## Types

- **Epic:** one per area (the summary is the area name only). Has no branch of its own.
- **Task:** one implementation unit; features are Tasks. About half a day of agent work; larger means
  split. Has a parent Epic.
- **Bug:** a defect with steps to reproduce.
- **Spike:** time-boxed research with a written output, never production code.

If the project lacks `Bug` or `Spike` issue types, use a Task with the label `bug` or `spike` and say so
once.

## Summary

Imperative, verb first, specific, at most 80 characters, no trailing period, no ticket key inside.
Good: `Add a per-project branch naming rule`. Bad: `Bot stuff`, `Improve login`.

## Formatting

- No emoji anywhere in a ticket.
- The body starts at `## Description`. No header line with Type, Size, Epic or Blocked by: those are
  fields and labels.
- Acceptance-criteria Result values are the plain words `Pending`, `Pass`, `Fail`.
- Labels: use only the labels listed in `docs/JIRA.md`. Never invent one without approval.

## Quality bar

Reject and rewrite a draft that has: no checkable acceptance criteria, no scope, vague words ("etc",
"improve", "handle edge cases") without a list, or more than one independently shippable behaviour.
Every criterion must be independently checkable by someone who did not write it.

## Steps

1. Decide the type. If the type, scope or acceptance criteria are missing, ask. Do not invent
   criteria the user has not seen.
2. Load `templates/<type>.md` from this skill's folder and fill every section. Replace every `<...>`.
   Write `None` where a section does not apply.
3. Find the parent epic (read-only search). If there is none and the type is not Epic, propose the
   Epic first. A Task never has no epic.
4. Search for duplicates by key words and show near matches.
5. Print the full draft: type, summary, parent epic, labels, priority, and the body exactly as it will
   be sent.
6. **Ask for explicit approval.** Creating a ticket is an outward write. Approval for one ticket does
   not cover another.
7. On approval, create it. Write temporary files in the scratchpad, not the repository.
8. Report the key and its URL.

## Batches

Epics first, then Tasks with their parents, in dependency order so `Blocked by` can link real keys.
Show the whole list and get one approval for exactly that list. Link each dependency afterwards.

## Never

- Create, edit, comment on or transition a ticket without approval.
- Create a project, change a workflow, or change Jira settings.
- Paste secrets or personal data into a ticket.
