# Phase C5: Library and templates

> **Built.** What it delivered, and what is still open, is in [status.md](status.md).

## Goal
Make Composers easy to start, repeat and share.

## Depends on
C3.

## In scope
- **Templates:** a few built in, such as "Triage and split into tasks", "Parallel refactor", "Review all open pull requests", "Fix failing checks across branches". Each fills in instructions, tools and limits.
- **Composer files:** a Composer can live in `<repo>/.circles/composers/<name>.toml`, shared with the team, or in the user folder. Same layering as settings.
- **Memory:** a Composer keeps short notes between runs (what it learned about a project), editable by the person, stored in the app folder and never sent anywhere else.
- **Run history:** past runs with their steps, children, cost and outcome. Re-run with the same input.
- **Steps as data (only if needed):** a simple list of steps in a Composer file (ask, spawn, wait, review) that the loop follows, for people who want a fixed process. Not a graph engine.

## Out of scope
Sharing outside the repository, a marketplace.

## Deliverables
- Templates, the file format, the history screen, the memory editor.

## Technical notes
- A shared Composer file may only name tools and limits. It can never turn a tier down.
- Memory is capped in size and shown in full before a run uses it.

## Done checklist
- [ ] A template creates a working Composer in one step
- [ ] A Composer file in the repository shows up for that project only
- [ ] A shared file cannot make a destructive tool silent
- [ ] Run history can re-run a past run
- [ ] Memory is visible, editable and capped

## Manual test script
1. Create a Composer from a template and run it.
2. Save it into a repository and open the project to see it.
3. Edit its memory and check the next run uses the edit.
