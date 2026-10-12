# Phase 2: App shell

## Goal
The three-column frame, the sidebar, routing, and the shortcut system. All with fake data. No backend.

## Depends on
Phase 1.

## In scope
- macOS overlay titlebar with traffic lights, sidebar toggle, back and forward buttons.
- Left sidebar (about 250px, resizable, collapsible): header, Home, Create, Search, "My workspaces" grouped by project with count and `+`, footer with version badge, help, settings gear.
- Center area with route outlet.
- Right panel (about 380px, resizable, collapsible), empty placeholder for now.
- `react-resizable-panels` for the columns. Sizes and collapse state persist.
- Router with history (back and forward work). Routes: `/`, `/workspace/:id`, `/settings/*`, `/design`.
- Zen mode (hides both sidebars).
- Central action registry, which feeds shortcuts, later the palette, and the shortcuts dialog.
- Shortcuts: Cmd+B or `[` left sidebar, Cmd+Opt+B or `]` right sidebar, Cmd+. zen, Cmd+/ shortcuts dialog, Cmd+Opt+T theme toggle.
- Shortcuts dialog grouped by Navigation, Workspaces, Chat, Review, Git, Terminal, App.
- Sidebar mock data: pinned, sections, unread (bold) state, active row indicator.

## Out of scope
Real data, create flow, palette UI (phase 6), settings content (phase 13).

## Deliverables
- `apps/desktop/src/components/shell/*`, `apps/desktop/src/routes/*`, `apps/desktop/src/stores/ui.ts`, `apps/desktop/src/lib/actions.ts`.

## Technical notes
- Use `data-tauri-drag-region` on the titlebar, and make sure buttons inside are not draggable.
- Shortcut handling in one hook with a conflict check. Ignore shortcuts while typing in inputs unless they are modifier combos.
- Sidebar must stay usable at its narrowest (the screenshot shows about 258px wide).

## Done checklist
- [ ] Sidebar and right panel resize, collapse, and persist
- [ ] Back and forward navigation works
- [ ] All listed shortcuts work and appear in the dialog
- [ ] Zen mode works and restores previous sizes
- [ ] Layout holds at minimum window size
- [ ] Both themes look correct

## Manual test script
1. Drag both dividers, quit, relaunch, confirm sizes persist.
2. Click a mock workspace, then back and forward.
3. Press every listed shortcut.
4. Resize the window to the minimum, confirm nothing overflows.
