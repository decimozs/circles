# Composers: where it lives in the UI

The app keeps the layout it has: a left pane, a center column with the top bar, and a right pane. A Composer reuses all three. Only what fills them changes.

## Left sidebar

```
Home
Create
Composers          <- new, below Create, opens the Composers home
Search

Pinned
  (pinned workspaces and pinned Composers, mixed, newest pinned first)

Composers          <- new group, same look as a project group
  Release captain          [+]   3 running
    Triage open PRs        2min
    Parallel refactor      1h
  Reviewer                 [+]
    ...

My workspaces
  project-one              [+]
    Dhaka                  5min
```

- **Composers nav item:** below Create. It opens the Composers home: the list of Composers, a "New Composer" button and the templates (from C5).
- **Composers group:** each Composer is a row that works like a project: a name, a `+` to start a new run, and its runs listed under it with the last-used time (same "3min" format as workspaces). Runs that need an answer show the same dot as an unread chat.
- **Pinned:** a Composer or a single run can be pinned. They appear in the existing Pinned section.
- **Children stay where they are:** workspaces a Composer starts appear in their normal project group, with a small badge linking to the run that made them. They are not moved into the Composer group, so nothing about workspaces changes.
- **Right-click and the `...` menu:** rename, pin, edit, duplicate, delete, same as other rows.

## Center column

A Composer run opens like a workspace chat:

- Top bar: the run title, the same tab strip pattern (runs of this Composer), and the terminal toggle is absent.
- The conversation uses the same message list and input. The input has the permission and model selectors of a chat, with the Composer's brain instead of a Claude model when it is not Claude.
- Tool calls show like the existing tool blocks, and approvals use the existing permission card.
- A run that is waiting shows a banner with the question.

## Right pane

The tabs change to ones that suit a Composer. The pane, its resize handle and its collapse stay the same.

- **Fleet:** every child workspace and chat with status, last activity, changes and an open link (C3).
- **Approvals:** what is waiting for an answer, across this Composer's runs (the global inbox arrives in C6).
- **Steps:** the run log, each model reply and tool call with its result and cost.
- **Details:** the Composer's instructions, limits and notes, and its **autonomy**: a list of its tools, each set to ask, allow or off, with a warning on any `external` or `destructive` tool set to allow. Editable.

The git graph at the bottom of the pane is hidden for a Composer, since it has no repository.

## Settings

A new page, **Composers**, in the group **Agents & environment**, after Environment. It holds what applies to all Composers:

- Default brain: provider and model.
- Provider API keys: set, test, remove. Values are never shown (from C4).
- Limits: children at once, children in total, steps per run, wall time, spend per run and per day.
- Autonomy defaults for new Composers: what each tier does. They start safe (everything above `read` asks). Each Composer can change its own.
- Unattended runs: the global pause and the daily cap (from C6).
- The tool server: whether it is running, and a reset of its token.

Settings that belong to one Composer (instructions, allowed tools and projects, its own limits) live in that Composer's Details tab and its edit dialog, not in Settings.

## Routes

- `/composers` the Composers home.
- `/composer/:id` a Composer, showing its latest run or an empty start screen.
- `/composer/:id/run/:runId` one run.
- `/settings/composers` the settings page.

## Where each part is built

| Part | Phase |
|---|---|
| Composers nav item, home, group in the sidebar, create and edit dialog, run page, steps and details tabs | C2 |
| Fleet tab, child badges, stop controls | C3 |
| Provider keys and model choice in settings | C4 |
| Templates on the Composers home, run history | C5 |
| Global approvals inbox, triggers settings | C6 |
| Audit view | C7 |
