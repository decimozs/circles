# Phase 6: Home, Create, Command palette

## Goal
The three entry-point screens, wired to real data. First phase where UI and backend meet.

## Depends on
Phases 2 and 5.

## In scope
- **Home:** header with title and "Create workspace", search input, archive filter ("Hiding archived" or "Showing archived"), workspaces grouped by recency (Today, Yesterday, Last 7 days, Older) with counts and relative times. First-launch empty state with Add project, Clone from GitHub, Quick start.
- **Create modal:** project picker, `···` menu (base branch override, create from existing branch or PR), prompt textarea ("What do you want to work on?"), model and effort picker, `+` attach, Create button (Enter). Submitting creates the workspace, navigates to it, and stores the first prompt to be sent in phase 8/9.
- **Command palette** (Cmd+K and the Search nav item): tabs All, Workspaces, Projects, Actions, Settings. Recent workspaces section. Fuzzy search. Every action shows its shortcut. Backed by the action registry from phase 2.
- Actions: Create workspace (Cmd+N), Create section, Add project (Cmd+Opt+A), toggle theme, toggle sidebars, toggle zen, open settings, archive workspace.

## Out of scope
Sending the prompt to Claude (phase 8), chat view (phase 9).

## Deliverables
- `apps/desktop/src/features/home`, `apps/desktop/src/features/create`, `apps/desktop/src/features/palette`.
- Create flow shows progress states (fetching, creating worktree, copying files).

## Technical notes
- Palette is built on `cmdk`. Use a single data hook that merges workspaces, projects, actions, and settings pages.
- Create modal remembers the last used project and model.
- Disable Create until a project and non-empty prompt exist, but allow creating with an empty prompt via a secondary action.
- Handle creation errors inline in the modal with a retry.

## Done checklist
- [ ] Home lists real workspaces with correct grouping
- [ ] Archive filter works
- [ ] Create modal creates a real workspace and navigates to it
- [ ] Palette finds workspaces, projects, actions, and settings pages
- [ ] Every palette action runs and shows its shortcut
- [ ] Empty states for no projects and no workspaces

## Manual test script
1. Fresh DB: confirm empty state, add a project from it.
2. Cmd+N, create a workspace, confirm progress and navigation.
3. Cmd+K, type part of a workspace name, open it.
4. Toggle the archive filter with one archived workspace.
5. Force a creation error (bad base branch), confirm the inline error.
