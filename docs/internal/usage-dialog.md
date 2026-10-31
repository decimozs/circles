# Usage for the LLM provider: plan

Replace the "Add project" button in the sidebar footer with a **Usage** button that opens a Usage
dialog: cost, tokens and limits for Claude Code, in the layout of the reference screenshot.
Status: plan, nothing built.

## Sidebar change

- `features/sidebar/sidebar.tsx`: the footer's `IconButton label="Add project" hint="⌘⌥A"` (folder
  plus icon) becomes `IconButton label="Usage"` with a chart icon. It opens the dialog.
- Add project is not lost: the empty sidebar still has its "Add project" button, and `Create` and the
  command palette still add projects. The `⌘⌥A` shortcut stays bound (palette and shortcuts dialog
  keep listing it); only the footer icon goes. Say so in `docs/shortcuts.md` if the docs mention
  the footer button.
- The new button gets its own shortcut hint only if one is free; otherwise none.

## The dialog (from the screenshot)

Header: `Usage` / `All environments` / `All providers` breadcrumb, range buttons `Past 24h`, `7 days`,
`30 days`, `90 days`, view buttons `Cost`, `Tokens`, `Limits`, refresh, close.

- **Cost view:**
  - Total estimated cost for the range and the number of sessions, labelled "API estimate".
  - One row per provider with its share and tokens. There is one provider (Claude Code), so one row.
  - A daily cost area chart with a hover readout.
  - Totals: processed tokens, cached input, uncached input, output, and cache savings.
  - Cost by type: a stacked bar and legend for input, cache read, cache write and output.
  - Breakdown, switchable between `Model` and `Day`: rank, model, cost, share, tokens, with a small
    bar under each model.
- **Tokens view:** the same chart and breakdown in tokens, not dollars.
- **Limits view:** the plan windows Claude Code reports (5-hour and 7-day utilisation, status, when
  the window resets, whether overage is in use), from the data Circles already reads.
- **Environments:** the second breadcrumb is the Claude config folder, such as `~/.claude` and
  `~/.claude-work`, since this user runs two. `All environments` is the sum.
- **Providers:** only Claude Code exists, so the filter is shown but has one entry, ready for more.

## Where the numbers come from

Circles stores one running cost per chat (`sessions.total_cost_usd`), with no tokens, model or date
per turn. That cannot draw this dialog. Two options:

| | A. Read Claude Code's own transcripts | B. Record usage per turn in Circles' database |
|---|---|---|
| Source | `<config dir>/projects/**/*.jsonl`: each assistant message has `model`, `timestamp` and `usage` (input, output, cache read, cache write, 5-minute and 1-hour cache) | the `result` event of each turn, saved to a new table |
| History | everything already on disk | only from the day it ships |
| Covers | Circles chats and anything else run through Claude Code | Circles chats only |
| Cost | a price table in Circles, so "API estimate" | the CLI's own `total_cost_usd` per turn, but only the total, not by type |

**Decision: A.** It has history from day one, gives tokens by type (needed for cost by type
and cache savings), and covers both config folders. The price is that cost is an estimate from a
price table, which the dialog says. A switch, "Only chats run in Circles", limits it to the Claude
session ids Circles has stored, so the total can match what Circles started.

## Backend (Rust)

- New module `usage/` in `src-tauri/src`:
  - `scan.rs`: find config folders (the resolved `CLAUDE_CONFIG_DIR` from the Environment setting
    plus `~/.claude*` that hold a `projects` folder), walk `projects/**/*.jsonl`, read each line,
    keep `type: assistant` lines with a `usage` object.
  - **Dedupe:** one API message can appear on several lines while it streams, so key by
    `message.id` (and `requestId`) and keep the last. Include subagent transcripts.
  - **Incremental:** remember `(path, size, mtime, byte offset)` and read only what was appended;
    store the day-bucketed totals in a small cache file in the app's data folder (or an SQLite table),
    so opening the dialog does not re-read hundreds of MB. First scan runs on a background task and
    streams progress.
  - `prices.rs`: a table of model id prefix to dollars per million tokens (input, output, cache read,
    5-minute write, 1-hour write), with a fallback "unknown model: tokens only". Kept in one file
    with the date it was last checked.
  - `aggregate.rs`: totals, per day, per model, per type, cache savings (what cache reads would have
    cost as plain input, minus what they cost), session counts.
- Commands in `commands/usage.rs`: `usage_summary(range, environment, only_circles)` and
  `usage_refresh()`. ts-rs types: `UsageSummary`, `UsageDay`, `UsageModel`, `UsageTotals`.
- The Limits view reuses `claude_rate_limit` and the stored `RateLimit` (`five_hour_utilization`,
  `seven_day_utilization`, `resets_at`). It shows "no data yet" until a turn has reported one.
- Local only: nothing is sent anywhere. Transcripts hold your prompts; the scan reads the `usage`,
  `model`, `timestamp`, `id` and `cwd` fields and keeps only numbers.

## Frontend

- `features/usage/`: `usage-dialog.tsx`, `cost-view.tsx`, `tokens-view.tsx`, `limits-view.tsx`,
  `daily-chart.tsx`, `cost-by-type.tsx`, `breakdown.tsx`, `api.ts`.
- **Chart:** no charting library is installed. A daily area chart, a stacked bar and thin bars are a
  small amount of SVG, so draw them by hand and keep the bundle light (a library would add tens of KB
  for one chart). Axis labels, hover readout, keyboard-reachable data table fallback for
  accessibility. The colour comes from the theme tokens.
- Dialog from `@circles/ui` (`dialog.tsx`), wide, scrollable body, state (range, view, grouping) kept
  in the component and the last range remembered in `localStorage`, in try/catch.
- Empty state: "No Claude Code usage found yet" with the folders that were searched.
- Money formatting by `Intl.NumberFormat`; tokens as `2.16B`, `15.9K`.

## Risks and checks

- **Prices go stale.** Cost is only as good as the table. The dialog says "API estimate". A test
  fails if a model seen in a fixture has no entry, so new models are noticed in review.
- **Plan users do not pay per token.** A subscription user's real cost is the plan; the number is
  what the same usage would cost on the API. The label says so, and "Cache savings" is shown the
  same way.
- **Cost of scanning.** A big history is slow the first time. Incremental reading, a background
  task, a progress line and a cancel on close keep the dialog responsive.
- **Format drift.** Claude Code's transcript format is not a public contract. The parser ignores
  lines it does not understand, reports how many it skipped, and has fixture tests from a real
  transcript (with the text stripped).
- **Limits** depend on the CLI emitting the rate-limit event; some plans and versions do not.
- Tests: Rust parser, dedupe, incremental read and price table with fixture files; aggregation by
  day and model; frontend rendering of each view from a mocked IPC result. The dialog's look, and
  the first scan on a real history, are checked by hand; the PR says what was and was not seen.

## Phases (stacked PRs)

1. **Scan and aggregate.** `usage/` with scanner, dedupe, price table, aggregation, command, types,
   tests. No UI.
2. **Sidebar button and Cost view.** Replace the footer button, the dialog shell, range and
   environment filters, totals, daily chart, cost by type, breakdown.
3. **Tokens and Limits views.**
4. **Docs.** `docs/usage.md` (new page, added to `docs/README.md`), `docs/shortcuts.md`,
   `docs/settings.md` if a setting is added, `CHANGELOG.md`, through `circles-update-docs`.

## Process

Jira is not set up, so each is a recorded "Ticket: None" exception, as with the recent changes.
