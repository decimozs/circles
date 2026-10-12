# Phase C6: Triggers and unattended runs

> **Built.** What it delivered, and what is still open, is in [status.md](status.md).

> **Required.** Composers must work both attended and unattended. Build this right after C3.

## Goal
Composers that start on their own, with guardrails.

## Depends on
C3.

## In scope
- **Triggers:** a schedule, a new or changed pull request or issue seen through `gh` polling, or a failing check on a branch. Each trigger belongs to one Composer and one project.
- **Unattended mode:** the run goes ahead with every tool the person set to `allow`. Anything set to `ask` waits in the **approvals inbox** instead of blocking.
- **Inbox:** one list of everything waiting for a person, across all runs, with a notification.
- **Guardrails for unattended runs:** lower limits by default, a daily cap on runs and spend, a quiet failure rule (stop and notify after repeated errors), and a global pause.
- **Quiet hours and the Mac asleep:** missed schedules run once on wake or are skipped, as chosen.
- **Keeping the app alive (open question 5):** an optional menu bar item that keeps Circles running after its window closes, an option to start at login, and an option to keep the Mac awake while an unattended run is active. All off by default and shown clearly in Settings.
- **Attended and unattended in one run:** a person can open an unattended run while it works, watch it, answer its approvals and take it over.

## Out of scope
Remote triggers, webhooks from outside this Mac, and a separate background service that runs while the app is closed. Unattended runs need Circles to be running.

## Deliverables
- Trigger settings per Composer, the inbox, the scheduler.

## Technical notes
- Text from issues and pull requests is the most likely carrier of injected instructions. Triggered runs treat it as untrusted data and never give it more authority than a person's message has.
- A trigger never fires twice for the same event.

## Done checklist
- [ ] A scheduled run starts at its time and stops at its limits
- [ ] An unattended run cannot do a step that is set to `ask` without approval
- [ ] The inbox shows waiting approvals and clears when answered
- [ ] Repeated failures stop the trigger and notify
- [ ] The global pause stops everything

## Manual test script
1. Add a schedule for a read-only Composer and watch it run.
2. Add a trigger on a pull request and open one.
3. Let an unattended run reach a push that is set to `ask` and check it waits in the inbox.
4. Press the global pause.
