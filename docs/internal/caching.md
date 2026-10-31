# Caching: show what is known now, update it in the background

Status: the Usage part (phases 1 and 2) is built. Decisions: usage history is kept after Claude Code
deletes a transcript; the read at launch is on by default with no setting; the file text store is capped
at 100 MB (not built yet). The shared GitHub cache (phases 3 to 7) is not built. One difference: the
Usage dialog has a **Rebuild history** link at its foot, not a Settings button.

Several screens wait for a slow answer every time they open, although what they show changed little
since the last time. The aim is one pattern for all of them: **show the last answer at once, say how
old it is, refresh in the background, and replace it when the new answer arrives.** The Usage dialog
is the first, and the worst.

## Why Usage loads

Measured on this Mac (971 transcripts, 790 MB, one of them 68 MB and growing as you work):

| What | Time |
|---|---|
| First scan after the app starts | 4.6 s |
| A second scan, nothing changed | 70 ms |
| A GitHub list call, for comparison | 0.9 to 1.8 s |

What the code does today:

1. **The scan cache lives in memory in the Rust process.** Each launch of Circles starts cold, so
   the first open of the dialog reads the whole history (4.6 s) behind a loading screen.
2. **The result is cached per range and per environment in the React query cache.** Choosing
   another range, or another Claude folder, is a new key with no data, so the loading screen shows
   again, although the data to answer it was read a moment ago.
3. **A file that grew is read again from the start.** The conversation you are working in is a
   transcript that grows on every message (68 MB here), so it is re-read whole each time.
4. **Nothing runs before you open the dialog.** The wait always happens in front of you.

I have not reproduced a loading screen on every open within one run; by the code, only the first
open after launch and a change of range or environment show it. If it happens more often for you,
that is a finding to chase before building, not something this plan explains.

## Usage: the design

A disk cache in the app's own database, filled incrementally, read in milliseconds.

- **Two tables** (one new migration):
  - `usage_files(path, len, mtime_ms, offset)`: how far each transcript was read.
  - `usage_messages(id PRIMARY KEY, model, at, day, session, input, output, cache_read,
    cache_write_5m, cache_write_1h)`: one row per answer. A message streamed several times is
    replaced by its last copy through the primary key. Cost is **not** stored: it is computed from
    tokens when read, so a corrected price applies to old rows too.
  - About 28,000 rows today, a few MB.
- **Incremental scan.** For each transcript: unchanged (same length and time) is skipped; grown is
  read from `offset` to the last complete line (a line still being written is left for next time);
  shorter or replaced is deleted and read again from the start; new is read whole. Reading a
  grown 68 MB file then costs the few KB that were added, not 68 MB.
- **Reading is separate from refreshing.**
  - `usage_summary` reads the database only and answers in milliseconds. It also returns when the
    data was last refreshed and whether a refresh is running.
  - `usage_refresh` runs the incremental scan on a background task, one at a time (a second call
    while one runs does nothing), and sends a `usage:changed` event with progress when it finishes.
- **Refresh before you ask.** A short while after the app starts, and when the dialog opens, a refresh
  starts unless one finished in the last minute. By the time you open the dialog it is usually
  current.
- **The dialog.**
  - Opens at once with the stored numbers, a line "Updated 3 min ago" and, while refreshing, a small
    spinner. No loading screen.
  - The numbers update when `usage:changed` arrives.
  - Changing the range or the environment keeps the old numbers on screen until the new ones come
    (React Query `placeholderData: keepPreviousData`), and with a database read that is a few
    milliseconds.
  - Only the very first run, with an empty database, shows progress ("Read 312 of 971 files"), and
    shows the numbers so far as they grow.
- **History survives Claude Code's clean-up.** Claude Code deletes transcripts older than 30 days by
  default, so today a 90 day range would quietly lose data. Rows are kept after their file is gone,
  so the page keeps the history it already saw. This is a decision for you (see questions): the
  alternative is to delete rows with their file so the page always matches what is on disk.
- **A way out.** If the database for this looks wrong, a "Rebuild usage history" button in Settings
  (or the guide's troubleshooting) deletes the two tables' rows and scans again. Rows from deleted
  files cannot be rebuilt, which the button says.
- **Tests.** Appended lines are picked up without re-reading the file; a half-written last line is
  not counted until it is complete; a replaced file is read again; a deleted file keeps or loses its
  rows as decided; two refreshes at once run once; the summary from the database equals the summary
  from a full scan on the same fixtures; the real-history test reports the second launch time. In the
  frontend: stored numbers show before the refresh answers, and a range change keeps the old numbers
  until the new ones arrive.

Expected result: opening the dialog takes a database read (tens of milliseconds), with no loading
screen after the first run ever.

## Other places that need the same

Measured where I could, with `gh` and `claude` on this Mac:

| Where | Cost now | How it changes | Proposal |
|---|---|---|---|
| Pull request list (`gh pr list`, one call per repository, 4 at a time) | 1.8 s per repository | Changes often | Keep the last list per scope and state on disk. Show it at once with its age, refresh in the background on open and when the window is focused again. Keep the old list while switching scope or state. |
| Pull request detail (`gh pr view`) | 0.9 s | An open one changes; a merged or closed one does not | Same, and a merged or closed one is served without asking again while the list says its `updated_at` is unchanged. |
| Files of a pull request and the text of each file side | 3 calls for the list, 1 per side per file | **Never** for a given commit | Cache by `(repository, base, head)` and `(repository, commit, path)`, with no expiry, in a size-capped store (200 MB, least recently used first). A file opened twice, or after a restart, costs nothing. |
| GitHub repository picker in Add project (`gh repo list` plus organisations) | 1.4 s plus 0.5 s each time the dialog opens | Rarely | Keep for 10 minutes, show at once, refresh in the background. |
| GitHub accounts card in Settings (`gh auth status`) | 1.5 s on every visit | Rarely, and after a switch (already refreshed) | Same, with the existing refresh after Switch account. |
| Claude account card (`claude auth status`) | 0.17 s | Rarely | Not worth it. |
| Workspace pull request status, checks, review threads, deployments (`gh` per workspace) | Not measured | Often | Measure first. If they are slow and shown on open, they are the next candidates. |
| Git graph, branch states, changes, diff | Local `git`, fast | Often | No. A cache would only risk showing wrong state. |
| Sidebar lists (projects, workspaces, composers) | Database reads, milliseconds | Often | No. |

I looked at every query in the frontend. The ones not listed are local reads or one-time checks.

## One mechanism for the GitHub ones

For the pull request and GitHub items, a small shared piece instead of one cache per feature:

- **`cache` table** in the app database: `key`, `value` (JSON), `fetched_at`, `bytes`, `last_used`.
  Typed helpers get and put a value, and a prune removes the oldest past a size or age limit.
- **Keys include the GitHub account** (`login`), so one account never sees what another fetched,
  and a change of account is a miss.
- **Commands return `{ value, fetched_at, refreshing }`.** A miss waits for GitHub as today. A hit
  answers at once and starts a refresh in the background, which updates the row and sends a
  `cache:changed` event with the key; the frontend invalidates that query. This is the same shape
  as Usage, so the screens treat them alike.
- **A "last updated" line** next to the refresh button on each of these screens, and the refresh
  button forces a fresh read.
- **Not a blanket persisted query cache.** Persisting every React Query result to disk would also
  persist workspace and chat state, which must be right, not quick. Only the listed keys opt in.

## Privacy and safety

- Caches live in the app's data folder on this Mac, in the same database as your workspaces. They hold
  pull request titles, descriptions and file text from private repositories, in plain text, as
  GitHub's CLI and Claude Code's own transcripts already do on this machine.
- Settings gets **Clear cached data** (the `cache` table and the file text store; not usage history,
  which has its own button).
- Cached items expire: lists after 30 days, others after 90, so a repository you lost access to does
  not stay readable for ever. A call that fails with "not found" or "no access" deletes its entry.
- Tokens are never cached. Nothing here is sent anywhere.

## Phases (stacked PRs)

1. **Usage database and incremental scan**, `usage_summary` from the database, `usage_refresh` and
   the event, refresh at launch. Backend only, with the tests above.
2. **Usage dialog**: stored numbers at once, "Updated … ago", spinner, keep numbers while switching,
   first-run progress. Docs and troubleshooting.
3. **Shared `cache` table**, `Cached` result type and event, prune, Clear cached data in Settings.
4. **Pull request list and detail** through it, with the age line and keep-while-switching.
5. **Immutable caches** for files and file text.
6. **Repository picker and GitHub accounts card.**
7. **Measure the workspace pull request calls** and decide.

## Questions for you

1. **Keep usage history after Claude Code deletes a transcript?** Recommended yes: the 90 day range
   then really covers 90 days. The cost is that the page can show usage whose conversation is gone.
2. **Refresh at launch**: after 5 seconds, in the background, for everyone. It reads files in the
   `.claude*` folders without being asked to open Usage; that is the point of it, but it is a read
   that happens by itself. Should it be a setting, off by default?
3. **Size of the file text store**: 100 MB (decided).
