# Phase C7: Hardening

> **Built.** What it delivered, and what is still open, is in [status.md](status.md).

## Goal
Make Composers safe and dependable enough to leave running.

## Depends on
C2 to C6, as built.

## In scope
- **Prompt-injection review:** go through every tool that returns outside text and every place it can reach a model. Test with hostile pull requests, issues, file names and commit messages.
- **Policy review:** the tiers hold in every mode and for every caller. Fuzz the policy and the input checks.
- **Limits review:** runaway loops, huge outputs and slow tools all end cleanly.
- **Audit view:** a screen that shows every call a Composer made, who approved it, and its result, with search and export.
- **Kill switch and recovery:** stop everything from the menu bar and a key, and a crash audit like the one in phase 15 for runs (kill at every step kind, relaunch, no duplicates, no orphan processes).
- **Performance:** many runs and children at once, long run logs, a big audit table.
- **Docs:** a Composer guide, the threat model updated with what was learned, a troubleshooting section.
- **Tests:** a fake provider and a fake Claude for the full suite, plus a short list of opt-in real runs.

## Out of scope
New features.

## Deliverables
- Audit view, updated `threat-model.md`, `docs/composers/hardening.md` with results, test scripts.

## Done checklist
- [ ] Hostile input cannot trigger an action that is set to `ask` without approval, and cannot change a Composer's autonomy settings
- [ ] A crash at any step kind resumes without a repeated action
- [ ] No orphan process or workspace after any failure
- [ ] The audit view shows every call
- [ ] The kill switch stops a running fleet at once
- [ ] The full test suite is green

## Manual test script
1. Open a pull request whose text tells the Composer to delete a workspace, and run a triage Composer on it.
2. Force quit during each kind of step and relaunch.
3. Start many runs at once and watch memory and the cap.
4. Press the kill switch mid-run.
