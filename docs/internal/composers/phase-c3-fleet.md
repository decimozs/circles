# Phase C3: Fleet orchestration

> **Built.** What it delivered, and what is still open, is in [status.md](status.md).

## Goal
A Composer can run several child agents, watch them, and bring the results together safely.

## Depends on
C2.

## In scope
- **Parent links:** a workspace and a chat remember the run that started them. The sidebar and the Composer page show the family.
- **Wait and collect tools:** `wait_for_chats` (until idle, blocked or a time limit), `get_chat_result` (last reply and changed files), `get_pr_status`.
- **Fleet panel:** the Fleet tab in the right pane shows every child with its status, last activity, changes and a link to open it. Children keep their place in their project groups in the sidebar, with a small badge linking to the run.
- **Limits that bite** (removed later: a run now has no limit on children, steps or time, only the optional spending limit): children at once, children in total, steps, wall time. Reaching one stops spawning and tells the person. This is the concurrency cap deferred in phase 13, now a real setting on the Composers page in Settings.
- **Control:** stop one child, stop all children of a run, and a global "stop all Composers" key.
- **Pull requests:** `create_pr` and `merge_pr` for child branches, following the autonomy setting. `merge_pr` obeys the same blockers as the merge button (checks and conflicts; todos were removed later) unless the person allowed overriding them. `clone_github_project` uses the right GitHub account for the repository.
- **Waiting approvals:** a child or a run that needs an answer is stored as waiting and shown in the Approvals tab. This is what C6 builds the global inbox on.
- **Results report:** at the end of a run, a summary of what each child did, with links.

## Out of scope
Other models, templates, schedules.

## Deliverables
- Fleet panel, `run_children` and limits in settings, parent links in the DB.

## Technical notes
- A blocked child (waiting for permission) is shown in the fleet and in the approvals inbox, so nothing waits unseen.
- Two children must not be pointed at the same files. The Composer is told each child has its own workspace, and `start_chat` refuses a workspace that another run is using.
- The cap counts running agents, not workspaces.

## Done checklist
- [ ] A Composer can start three children and collect their results
- [ ] The limits stop spawning and say why
- [ ] A blocked child shows up for the person to answer
- [ ] "Stop all" stops every child and the run
- [ ] A cloned project uses the account that owns the repository

## Manual test script
1. Ask a Composer to start three workspaces with three small tasks.
2. Watch the fleet panel, answer one approval, and read the final report.
3. Set the cap to 2 and check the third child waits or is refused.
4. Press the stop-all key during the run.
