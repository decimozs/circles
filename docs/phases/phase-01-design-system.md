# Phase 1: Design system and theme

## Goal
One visual language, defined as tokens, that works in light, dark, and system modes. Every later screen builds only from this.

## Depends on
Phase 0.

## In scope
- Design tokens as CSS variables: background, panel, popover, border, muted, accent, destructive, success, warning, diff-add, diff-remove, terminal colors.
- shadcn Vite `ThemeProvider` with `light | dark | system`. Persist the choice. Follow `prefers-color-scheme` live when set to system.
- Typography: Inter for UI, JetBrains Mono for code and terminal. Base size 13px. Type scale.
- Radii, spacing, shadow, and focus ring tokens.
- Base shadcn components installed and themed: button, input, textarea, select, dropdown-menu, dialog, popover, tooltip, tabs, switch, badge, scroll-area, separator, command (cmdk), sonner toast, context-menu.
- A hidden `/design` route showing every token and component in both themes.

## Out of scope
App layout, real screens, icons beyond lucide.

## Deliverables
- `apps/desktop/src/styles/tokens.css`, `apps/desktop/src/components/theme-provider.tsx`, `apps/desktop/src/components/ui/*`.
- Theme toggle component (three-way: light, dark, system).
- `/design` gallery.

## Technical notes
- Dark reference values (from the target screenshots): app bg about `#141414`, panel about `#1a1a1a`, hairline border about `#262626`, active row about `#222`.
- Light theme must be designed, not inverted: check contrast (WCAG AA) for text and borders.
- No hard-coded colors in components. Use tokens only. Add a lint rule or grep check.
- Prevent flash of wrong theme on load by setting the class before React mounts.

## Done checklist
- [ ] Toggle switches light, dark, system, and persists across restarts
- [ ] System mode follows macOS appearance live
- [ ] No flash of wrong theme on launch
- [ ] `/design` gallery looks right in both themes
- [ ] Contrast checks pass
- [ ] No hard-coded hex values outside `tokens.css`

## Manual test script
1. Open `/design`. Cycle the toggle through the three modes.
2. Change macOS appearance while in system mode, confirm live switch.
3. Restart the app, confirm the choice persists.
4. Tab through components, confirm visible focus rings.
