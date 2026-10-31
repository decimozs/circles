# Chat with a glass message box: plan

The message box floats over the conversation and blurs what scrolls under it, as in the reference
screenshot. Today the box sits below the list in a column, so the list ends above it. Status: built. Not seen in the running app: the blur, contrast and scrolling speed.

## Target (from the screenshot)

- The conversation fills the whole pane. Messages scroll behind the box and show through it, blurred.
- The box is a rounded, semi-transparent panel with a thin light border and a soft shadow.
  Placeholder: "Ask anything. @ files, $ skills, / commands" (today's wording is kept if it differs).
- Bottom row inside the box: attach icon on the left; on the right the model chip with its icon and
  effort ("Sonnet 5.5 High") and a round send button.
- A "Scroll to end" pill with a small chevron floats just above the box, centred, while the view is
  not at the bottom (today a round arrow button at the bottom of the list).
- User messages stay as muted rounded bubbles on the right; replies stay plain text. No change.

## Why it is not just a CSS change

The list and the box are siblings in a column (`chat-panel.tsx`: `MessageList`, then the revert
banner, the error line, run approvals, cards panel, side pane and `Composer`). To scroll under the
box, the box must be positioned over the list, and the list must leave room at its end so the last
message can scroll clear of the box.

## Design

1. **A bottom dock.** In `chat-panel.tsx`, wrap everything that sits under the list (banner, error,
   approvals, cards, side pane, `Composer`) in one container, `absolute inset-x-0 bottom-0`, the
   pane being `relative`. The container is `pointer-events-none` with `pointer-events-auto` on its
   children, so the empty strip beside the box does not block clicks on messages.
2. **Measured height.** A `ResizeObserver` on the dock writes its height to a CSS variable
   (`--dock-height`) on the pane. The list's scroll area gets `padding-bottom: var(--dock-height)`
   plus a little air. The box grows with its text, the queue and the cards, and the padding follows.
3. **The glass.** On the box itself (`composer.tsx`, the `rounded-xl border` div):
   - background a translucent card colour, for example `bg-card/60`;
   - `backdrop-blur-xl backdrop-saturate-150`, with the `-webkit-` form Tailwind emits for WKWebView;
   - `border-white/10` in dark and `border-black/10` in light, a soft `shadow-lg`;
   - larger radius (`rounded-2xl`) and a focus ring on `focus-within`, as now.
   Colours come from the existing tokens in `packages/ui` so both themes work, not hard-coded values.
4. **Readability.** Behind the box a short gradient fade (transparent to `background`) so text does not
   run into the box edge, and a stronger blur or opacity in light mode, where text under a pale
   panel is harder to read. Under `prefers-reduced-transparency` the box is opaque.
5. **Scroll to end.** Move the pill into the pane, `absolute`, `bottom: calc(var(--dock-height) + 0.75rem)`,
   pill shape with the chevron and the words "Scroll to end", same button and handler as today. The
   conversation map on the right edge is unchanged.
6. **Stick to the bottom.** The list's "at the bottom" check uses the scroll element's size, so the
   padding counts as scrollable space. A new user message still scrolls to the end. A test covers the
   last message ending above the box.
7. **Both places.** `Composer` is used with `variant="workspace"` and `variant="composer"`; both get
   the glass, since they share the component.
8. **Popups.** The `@` and `/` suggestion list opens above the box. It stays an opaque `popover`
   panel, so it is not hard to read over the list.

## Files

- `apps/desktop/src/features/chat/chat-panel.tsx`: dock wrapper, height variable.
- `apps/desktop/src/features/chat/composer.tsx`: glass classes, the bottom row layout.
- `apps/desktop/src/features/chat/message-list.tsx`: bottom padding, the pill.
- `packages/ui` styles only if a shared glass utility is cleaner than repeating classes (decide while
  building; a single `.glass` utility used in two places is fine, one used once is not).
- `docs/chats.md` and `CHANGELOG.md`: a line, through `circles-update-docs`.

## Risks

- **Scrolling performance.** A large blur over a long, moving list can be slow in WKWebView. The list
  is already virtualised, which helps. If it stutters, lower the blur radius or the area, not the
  design.
- **Contrast.** Message text under the box is meant to show through but must not make the box text
  hard to read. Checked in light and dark by eye, with the placeholder and typed text.
- **Layout regressions.** Anything else that assumed the box is below the list (the side pane, the
  cards, the revert banner, the Composer page's own panels) moves into the dock; each is checked.
- **Tests.** jsdom has no layout, so `ResizeObserver` is mocked and the padding is asserted from the
  mocked height; the look itself cannot be tested there.

## Scope and PR

One PR, a single logical change (the chat layout), no behaviour change to sending, queueing or
attachments. It is stacked on the latest `origin/main`. Checks: `pnpm lint`, `pnpm typecheck`,
`pnpm check`, `pnpm test`.

## Verification

Run the app (`pnpm tauri dev`) and look at: a long chat, a short chat, a Composer chat, light and
dark, a multi-line message, queued messages, the `@` list, a permission card, the revert banner and
"Scroll to end". I cannot judge the glass from tests, so the PR says what was seen and what was not.

## Process

Jira is not set up, so this is a recorded "Ticket: None" exception like the recent changes.
