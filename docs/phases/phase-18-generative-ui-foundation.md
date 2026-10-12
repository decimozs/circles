# Phase 18: Generative UI, the foundation

Status: G0 to G5 built and tested (2026-10-11). See "Built" and "Gaps" at the end.

## Goal

An agent can put a small, structured interface in the chat and get your answer back as data:
a checklist of tickets to pick from, a plan to tick and reorder, a form for missing details.
This phase builds only the foundation: the vocabulary, the renderer, the request, the reply and
the guard rails. The features that use it (ticket picker, plan review, dashboards) come after.

## Where this sits among the known approaches (research)

| Approach | What it is | Fit |
|---|---|---|
| **Controlled** | The agent picks from components the app already has | **The base of this phase** |
| **Declarative, such as A2UI** (Google, v0.9 in April 2026) | The agent sends a JSON description. The app owns a catalogue of trusted components and ignores anything else | **Same idea, and the shape I borrow**: a client-owned catalogue, plain data, nothing executable |
| **MCP Apps** (`ui://` resources, in Claude, ChatGPT, VS Code) | A tool ships HTML shown in a sandboxed iframe | **Not now.** It is open-ended, needs an iframe sandbox and is for third-party tool UIs. Revisit if outside tools should bring their own UI |
| **Open-ended** (model writes UI code) | Arbitrary code in a sandbox | **Never** in a tool that can change a repository |

Nothing here is a ratified standard yet (a W3C community group is exploring one). So Circles
owns its small catalogue and keeps it close enough to A2UI that a mapping stays easy later.

## What exists and is reused

- **The tool pipeline** (`tools/`: catalogue, schema validation, tiers, audit) and the socket
  to the helper process. Composer runs and, since the Jira work, ordinary chats both reach it.
- **Approval cards** (`approval-card.tsx`, `composer/dispatch.rs`): a tool call that waits for a
  person and comes back with their answer. The same wait, timeout and "expire on drop" logic
  is the model for a card that waits.
- **The Markdown renderer** and the chat timeline.
- **`ts-rs`** to keep the Rust and TypeScript types the same.

## Design

### 1. The catalogue (vocabulary)

A card is `{ id, title?, body: Component[] }`. Components for v1, each with strict fields and limits:

| Component | Purpose | Returns |
|---|---|---|
| `text` | A paragraph. Markdown subset, no HTML | nothing |
| `key_value` | Rows of label and value | nothing |
| `list` | Items with an optional detail line | nothing |
| `table` | Up to 8 columns, 50 rows | nothing |
| `code` | A block of text, with a language label | nothing |
| `choice_list` | One or many options, each with a label and detail | the chosen values |
| `form` | Fields: text, long text, select, checkbox, number | the field values |
| `steps` | A plan: steps you can tick, remove or reorder | the steps, in order, with their state |
| `confirm` | A question with named answers | the answer |
| `progress` | A bar or a list of states | nothing |
| `actions` | Buttons | the button's value |

Limits: depth 2, 30 components, 2,000 characters per text, 100 options. A card is at most 32 KB.
Every string is plain text. Links are `https` only and show their address.

### 2. The renderer

`apps/desktop/src/features/genui/` draws each component with Circles' own UI kit, so it looks like the rest of
the app, works with the keyboard and passes the same accessibility checks. A component that fails
validation shows a plain error card and never throws into the chat. An answered card is shown as
answered, with what was chosen, and its controls are off.

### 3. The request (channel)

A tool, `show_ui`, in the existing pipeline:

- `show_ui({ card, wait })`. With `wait: true` (the default when the card has input) the call
  **waits** for the person, as an approval does, and returns the answer. With `wait: false` it
  returns at once ("shown") and the card is only for reading.
- **Tier: read** (it changes nothing). The wait is limited (default 24 h for Composers, which are
  already given a long tool timeout, and 10 minutes for chats, with a clear "not answered" result).
- Reaches Composers through the existing Composer socket and ordinary chats through the chat
  door that Jira uses, with a new scope. The scope is only added when the setting is on.

### 4. The reply (return path)

The tool result is JSON: `{ "id": "pick-tickets", "status": "answered", "values": ["ROS-237"] }`,
or `"status": "dismissed"` or `"expired"`. A card is stored in a table `ui_cards` (id, chat,
Composer, the card, status, the answer, times). Pending cards expire when the app restarts, as
approvals do, and the waiting call is told so. A card can be answered once: the answer is a
compare-and-set, so two windows cannot both answer.

**Buttons never run code or tools.** In v1 a button returns its value to the model. The model
then calls whatever tool it needs, under the usual permission rules. (A button that calls a
tool directly is a later, separate decision.) The one other action is "open this workspace".

### 5. Guard rails

- Text that came from tools (tickets, pull requests) put into a card is shown as text, and the
  card is marked "includes text from outside" when the agent says so.
- Only the catalogue. An unknown type, an oversize card or a bad field is an error returned to the
  model, so it can fix it. Nothing is drawn.
- No HTML, no images from the web, no scripts, no styles from the model.
- Cards and answers are in the audit log. A card that asks for secrets (a field named like a
  password or token) is refused.
- A rate limit: at most 3 pending cards per chat, and one new card per second.
- The setting "Let agents ask with forms and choices" (on for Composers, off for chats until
  the first features exist).

## Phases

| Phase | What | Output |
|---|---|---|
| G0 | Spec: the catalogue (fields, limits), the threat model, the wait and expiry rules. Examples for every component | `docs/genui/spec.md`, `threat-model.md` |
| G1 | Types and validator: Rust (serde, limits, errors the model can read), TypeScript types by `ts-rs`, shared fixtures, tests with hostile cards | validated cards |
| G2 | Renderer and components, a test page with every component in both themes, accessibility checks, keyboard use | draws cards |
| G3 | Channel and storage: `show_ui` tool, `ui_cards` table (migration), wait and answer path, expiry on restart, events to the UI, audit | works end to end with a fake model |
| G4 | In the app: the card in the timeline at its call, a "waiting for you" strip above the input (like approvals), the sidebar dot, notifications when the app is in the background, answered state, dismiss | visible and usable |
| G5 | Teaching the agents: a short system prompt text with when to use cards and two examples (Composers and, behind the setting, chats), the setting, hardening, docs, manual guide, a real-Claude test | done |

Order: G0, G1, G2 in parallel with G3, then G4, G5.

## First features that this unlocks (a later phase, not this one)

1. **Ticket picker:** the Jira Composer shows a checklist and a "Start selected" button.
2. **Plan review:** a plan as steps you can tick and reorder before work starts.
3. **Richer approvals:** the diff summary, checks and todos inside the approval card.

## Risks

- **A model that overuses cards** and interrupts. Rate limit, the setting and the prompt text.
- **Small models write bad JSON.** The validator's errors are written for a model to read, and a
  card is never half drawn.
- **A wait that blocks an agent** for hours. Timeouts, a clear "waiting for you" in the sidebar,
  and a Dismiss.
- **Two sources of truth** (the card on screen and the stored one). The stored one is the truth,
  and the screen follows events.
- **Catalogue creep.** Every new component is a security review. v1 stays small on purpose.
- **Drift from a future standard.** Kept close to A2UI's shape so a mapping is possible.

## Decisions for you

1. **Blocking by default?** A card with inputs waits for you, like an approval (recommended), or
   the agent carries on and your answer arrives as your next message.
2. **Chats too, or Composers only first?** Recommended: Composers first, chats behind a setting.
3. **Can a button call a tool directly?** Recommended: no in v1. It returns a value and the
   model acts under the usual permissions.

## Sources

- A2UI, declarative generative UI with a client-owned catalogue:
  https://developers.googleblog.com/a2ui-v0-9-generative-ui/
- Overview of patterns (controlled, declarative, open-ended):
  https://copilotkit.ai/blog/the-developer-s-guide-to-generative-ui-in-2026
- MCP Apps (`ui://` resources in sandboxed iframes): https://mcpbundles.com/blog/2025/11/24/mcp-apps-interactive-ui-protocol
- W3C Generative UI community group: https://www.w3.org/community/gen-ui

## Decisions taken

1. **Blocking by default:** a card with inputs waits for the person, like an approval. A card with
   no inputs, or `wait: false`, returns at once with `shown`.
2. **Composers first:** on for Composers, off for ordinary chats until switched on in Settings.
3. **Buttons return values.** A button never runs a tool. The agent acts on the value under the
   usual permissions.

## Built (2026-10-11)

- **G0/G1:** `apps/desktop/src-tauri/src/genui/mod.rs`: the catalogue (text, key_value, list, table, code,
  choice_list, form, steps, confirm, progress, actions), limits (30 components, 32 KB, 100 options,
  50 table rows, 12 fields, 30 steps, 2,000 characters), the validator with errors written for the
  agent, and `check_answer`, which refuses any value the card did not offer.
- **G2:** `apps/desktop/src/features/genui/card-view.tsx`: every component drawn as plain text with the app's
  own controls. Markup is shown as text and never as elements (tested).
- **G3:** the `show_ui` tool (read tier) in `catalog::get`; `genui/store.rs` and migration 0013
  (`ui_cards`); `genui/service.rs` (wait, answer once, dismiss, time out, expire when the call goes
  away or the app restarts, three waiting cards and one new card a second at most); commands
  `genui_answer`, `genui_dismiss`, `genui_cards`, `genui_pending`; `genui:changed` events; the
  audit log gets the call and the answer.
- **G4:** cards above the message box (`cards-panel.tsx`), the waiting ones first and the last
  answered ones kept in view; "Waiting for you" on the Composers page; waiting cards count in the
  sidebar's waiting dot and in the run list; a notification when the app is in the background.
- **G5:** a paragraph in a Composer's prompt (only when cards are on and a person is watching);
  settings `genui.composers` (on) and `genui.chats` (off); chats get the tool through the same
  helper door as Jira, with a 10 minute wait.

## Gaps

- **Not inline in the timeline.** Cards sit in a strip above the message box, not at the point in
  the conversation where the agent asked. Answered cards stay in the strip, and the last two show.
  Putting them in the message list needs changes to the timeline builder.
- **No real-Claude run through the helper** with `show_ui`. The dispatcher, the service and the
  renderer are tested with fakes, and the helper path is the one that Jira already uses.
- **Not tried by eye** in the running app, in both themes.
- **Unattended runs are not told about cards**, on purpose: nobody is there to answer. A card in
  an unattended run waits up to 24 hours, and shows up under "Waiting for you".
- **Charts, saved views and buttons that call tools** are not in v1, as planned.
- **No sorting of a long list** inside `choice_list` (100 options at most, no search).
- **A reordered plan is sent as asked,** but the agent has to honour it: nothing makes it.
- **Chats started before the setting was switched on** do not get the tool until they restart.
