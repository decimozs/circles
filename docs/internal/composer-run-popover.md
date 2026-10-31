# A run popover instead of the right pane for Composer chats

Status: plan, nothing built.

A Composer's page has a right pane with four tabs (Fleet, Approvals, Steps, Details). It is replaced
by a small popover from the top bar, laid out like the T3 Code one in the reference screenshot:
where the work runs, which editor opens it, the branch, the pull request, and what changed.
Workspace chats (not Composers) keep the right pane, with Files, Changes and Checks.

## What the reference shows, and what it means here

The reference popover has, top to bottom:

1. This machine, with a folder button, and a chevron.
2. **Open in VS Code**, with a chevron to pick another editor.
3. **Add project script.**
4. The branch (`develop`), with a chevron.
5. The pull request: its title, `#904` and a checks mark.
6. **Create PR**, with a chevron.
7. **Changes**, `+26,617 -2,670`.

Rows 4 to 7 describe one git checkout. A Composer does not have one. Checked in the code
(`composer/store.rs`): a Composer's own workspace is a plain folder (branch `-`, base `-`, no
repository). The checkouts are the **workspaces its runs start**, which today are listed in the
Fleet tab. So the popover for a Composer chat is:

- **Top:** the run itself, like row 1: this Mac, and the run's folder.
- **One block per workspace the run started**, each with rows 2 and 4 to 7: open it in an editor,
  its branch, its pull request, the next step (Create PR, Respond to review, Merge, ...), and its
  changes.
- **Foot:** links to what the right pane held, so nothing is lost (below).

"Add project script" belongs to a project, not a run, and already has a place in Settings. It is left out
of this popover.

## Where the right pane's content goes

| Was in the pane | Goes to |
|---|---|
| **Fleet**: the tree of workspaces and agents, status, link to open | The workspace blocks of the popover replace the tree. A status dot and an **Open** link stay on each block. **Expand** (the fleet graph) stays as a dialog from the popover's foot. |
| **Approvals**: pending and answered | Pending ones are already shown above the message box (`RunApprovals`). The top-bar button shows how many wait, as a badge. The answered list opens in a dialog from the popover's foot. |
| **Steps**: every reply and tool call with cost | A dialog from the foot. |
| **Details**: instructions, notes, autonomy, repositories, integrations | The existing Composer settings dialog, opened from the foot. |

## Design

- **Trigger.** In the Composer top bar, the **Toggle right panel** button becomes the popover button
  (a list icon, with a badge for approvals waiting). Cmd+Option+B opens and closes it on a Composer
  page. Nothing else changes for workspace pages.
- **The pane is gone for Composers.** `app-shell.tsx` shows the right pane only on workspace pages;
  `RightPanel` loses its Composer branch; the Composer's width and open state are no longer
  remembered.
- **The popover** is the existing `Popover` from `@circles/ui`, about 22rem wide, opening below the
  button and aligned to its right edge, with a maximum height and its own scroll for a long run.
- **Which run.** The active run's tab, as the pane does now (`useFleet(composer.id, runId)`).
- **Rows** are the reference's: icon, label, optional chevron that opens a small menu (editor list,
  branch details, PR actions). Rows that do nothing do not look like buttons.
- **Per workspace**, using what exists:
  - editor: `editor_list` and `editor_open` for the workspace folder, remembered editor first;
  - branch and base, commits ahead, pull request number and state: `workspace_branch_states` (one
    call for all workspaces);
  - pull request title and checks, and the next step: the same readiness and next-action logic as the
    workspace top bar (`pr/next-action.tsx`), reused, not rewritten;
  - changes: the totals of the workspace's changed files; clicking opens the workspace on its Changes
    tab.
- **Cost of asking GitHub.** Readiness is a `gh` call of about 0.8 s per workspace. A run can start
  many. So the popover asks only while it is open, only for workspaces that have a pull request, a
  few at a time, and shows the branch and changes (local, instant) first. Readiness goes through
  the cache from the caching work so a second open is instant.
- **No new backend commands** are expected. If the totals of changes need one call per workspace that
  proves slow, a `workspace_summaries(ids)` command that returns branch, ahead, changes and pull
  request number in one go is the fallback. This is checked while building, not guessed here.

## Phases (stacked PRs)

1. **Move, don't lose.** Remove the right pane for Composer pages; add the top-bar button with the
   approvals badge and the popover shell; add the foot links that open Fleet graph, Approvals, Steps
   and Details as dialogs (the existing tab bodies, moved into dialogs). The popover lists the
   workspaces with name, status and Open. Nothing is lost at this point and nothing is new.
2. **Branch, editor and changes** per workspace: editor menu, branch row, changes totals and the
   link to the Changes tab.
3. **Pull request and next step** per workspace: title, number, checks mark, and the next-step row
   with its menu, through the cache.
4. **Docs and shortcut**: `docs/composers.md` (the right pane described there is gone),
   `docs/shortcuts.md`, `CHANGELOG.md`.

## Tests

- A Composer page has no right pane and no panel toggle; the button opens the popover; the badge
  counts pending approvals; Cmd+Option+B toggles it on a Composer page and still toggles the pane on
  a workspace page.
- The popover lists one block per workspace of the active run, switches with the run tab, and says
  so when the run has started nothing.
- Each foot link opens its dialog with what the tab held (existing tab tests are moved, not
  dropped).
- Per workspace: editor menu calls `editor_open` with that workspace; branch and ahead count; changes
  totals and the link; a pull request shows title, number and checks; no pull request shows Create PR;
  an error from GitHub shows in that block only, not in the others.
- Rows that are not actions are not buttons (a11y).
- Not testable here: how it looks and feels with a long run; checked by hand.

## Risks and choices

- **Less at a glance.** The pane showed the fleet tree and approvals all the time. The popover shows them
  on demand, with a badge for approvals as the one thing that must not be missed. If that proves too
  little, the popover can be pinned open later.
- **A run with many workspaces** makes a long popover. Blocks are collapsed to one line (name, status,
  changes) past the first three, and expand on click.
- **The screenshot is from another product.** The rows are matched in order and wording; colours
  and sizes come from Circles' own tokens, not from the image.

## Questions for you

1. **Where should Approvals, Steps and Details live?** Recommended: dialogs from the popover's foot,
   as above. The alternative is a second button beside the popover for each, which is more visible
   and noisier.
2. **Keep the popover for workspace chats too?** Recommended: no. Workspaces have Files, Changes and
   Checks in the pane, and a branch and a pull request of their own in the top bar already.
3. **Open in editor for the Composer's own folder?** Recommended: leave it out. It holds notes, not
   code, and the per-workspace blocks open the code.
