# Phase C0: Spec and threat model

> **Built.** What it delivered, and what is still open, is in [status.md](status.md).

## Goal
Decide what a Composer is, what it may touch and how it can go wrong, before any code.

## Depends on
Circles phases 0 to 15.

## In scope
- Answer the open questions in the README and record the answers. Questions 1, 2 and 4 are already answered: Claude only for now, Composers can create projects and workspaces, and merging is allowed depending on what the person wants. Question 3 is answered too: unattended runs are a must. Question 5 is answered too: the menu bar option, with no background service. All open questions are answered.
- **Tool catalogue:** every tool with its inputs, outputs, tier and what it wraps. First list:
  - `read`: `list_projects`, `list_workspaces`, `get_workspace`, `get_chat_tail`, `get_changes`, `get_checks`, `get_pr_status`, `get_git_graph`.
  - `write`: `create_workspace`, `start_chat`, `send_message`, `interrupt_chat`, `add_todo`, `rename_workspace`.
  - `external`: `push_branch`, `create_pr`, `comment_on_pr`.
  - `destructive`: `archive_workspace`, `delete_workspace`, `merge_pr`, `remove_project`.
  - Project tools, gated: `add_local_project`, `clone_github_project`.
- **Threat model:** runaway spawning, cost, and prompt injection (a pull request comment, issue, file or commit message that tells the Composer to do something harmful). List each threat with the mitigation and the phase that builds it.
- **Run and step data model** on paper: tables, states, what resume means for each kind of step.
- **Autonomy rules:** the per-tool `ask`, `allow` and `off` settings, their defaults, the per-project rules, what "allow for this run" covers, which merge blockers can be overridden, and the warnings shown when a person allows an `external` or `destructive` tool.
- **Limits (all but the spending limit removed later):** steps per run, children at once, children in total, wall time, spend.
- **Attended and unattended:** write down how each tier behaves in both modes. In unattended mode a tool set to `ask` waits in an inbox and the run goes on with whatever else it can do, or pauses.
- Name the screens: Composers list, Composer page (chat, fleet, approvals), edit dialog.

## Out of scope
Any code.

## Deliverables
- `docs/composers/spec.md` with the catalogue, the data model, the threat table and the decisions.
- `docs/composers/threat-model.md`.

## Technical notes
- Treat everything a tool returns as untrusted text. It is data for the model, never instructions with authority.
- A tool takes typed fields, never a free shell string.
- A tool result never contains secrets: environment values, tokens and key files are filtered out.

## Done checklist
- [ ] Every open question in the README is answered in writing (all are now)
- [ ] Every tool has a tier, inputs and an owner service
- [ ] Each threat has a mitigation and a phase
- [ ] The step state machine covers a crash at every state
- [ ] The owner has reviewed and approved the spec
