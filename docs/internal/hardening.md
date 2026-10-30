# Hardening audit

Results of the phase 15 audit. "Verified" means checked by an automated test or by reading the code.
"Manual" means it needs a person on a real Mac and has not been done yet.

## Security

| Item | Result |
| --- | --- |
| Path traversal | Verified. Every file command goes through `files::resolve_inside`, which rejects `..`, absolute paths and symbolic links that leave the workspace, and has tests for each. Attachment names are reduced to a safe file name (`chat::safe_name`). |
| Command injection | Verified by reading every `Command::new`. All spawns pass arguments as arrays (`git`, `gh`, `claude`, `open`, `osascript`, `ps`, `scutil`). A shell is only used for the login-shell environment lookup, the terminal itself and the setup, run and archive scripts, which are commands the user wrote. AppleScript text is escaped (`system::notification_script`, tested). |
| Option injection | Fixed in this phase. A branch or ref starting with `-` is refused before it reaches `git fetch`, `git branch`, `git worktree add`, the base branch setting and workspace creation (`git::not_an_option`, tested). |
| Tauri capabilities | Reviewed. The main window has `core:default`, window dragging and maximise, the folder picker, and the opener limited to `http(s)` URLs plus the macOS privacy settings URL. No filesystem, shell or process plugin is granted to the web view. |
| CSP | Set in `tauri.conf.json`: `default-src 'self'`, no remote scripts, `object-src 'none'`, workers from `self` and `blob:` (Monaco), images from the app, `data:`, `blob:` and GitHub avatars, `connect-src` limited to the Tauri IPC. `style-src` allows inline styles because Monaco, Base UI and the panels set styles at runtime. A separate `devCsp` allows the Vite dev server. **Manual:** open the built app and confirm the editor, diff view, terminal and avatars still work. |
| Secrets in logs | Verified. `logging::Redactor` scrubs the values of environment variables that look like secrets before anything is written. `scripts/scan-secrets.sh` greps the logs and database for token shapes (GitHub, Anthropic, Slack, AWS, private keys, bearer tokens). It found nothing in the development data. |
| Secrets in terminal history | Only script tabs keep their output on disk (free terminals do not). That output has known secret values removed before it is saved (`Terminals::scrub`, tested), and `scripts/scan-secrets.sh` covers the scrollback files. Secrets that are not known environment values (typed by hand, for example) are not recognised. |
| Secrets in settings UI | Environment variable values are masked and are never logged. Only the key path of a changed setting is logged. |
| Diagnostics | The "copy diagnostics" report passes through the same redactor. |
| Bypass mode | Off by default. The settings page and the first-run screen both warn about it. |
| Deep links | Circles registers no URL scheme and handles no deep links, so there is no input to validate. |
| Updater keys | No updater is configured yet. Keys must be generated and kept outside the repository when it is. |

## Crash and recovery

Covered by tests on the pieces that matter at each lifecycle point:

| Point | Test |
| --- | --- |
| Creating, setup interrupted | `workspace::service::tests::a_setup_script_cut_short_by_a_restart_leaves_a_ready_workspace_with_a_warning`, `reconcile_handles_interrupted_and_stale_states` |
| Working | `harness::store::tests::sessions_left_working_by_a_crash_are_marked_interrupted`, `harness::sessions::tests::startup_marks_chats_that_were_working_as_interrupted`, `a_crash_mid_turn_interrupts_the_session_and_explains_why` |
| Blocked on a permission | `interrupting_while_waiting_for_approval_cancels_the_request` |
| Orphan processes | `process::tests::reaps_live_orphans_and_ignores_reused_pids` |
| Missing worktree | `reconcile_marks_missing_worktrees_and_restore_repairs_them` |

**Manual, not done yet:** force quit with five running agents and relaunch, and kill the app
while a workspace is being archived. Do this on a real Mac before a release (see the checklist).

## Performance

**Manual, not measured yet:** startup time, memory with ten workspaces, scrolling a long chat,
a very large diff, terminal throughput. Record the numbers here when measured.

## Accessibility

- `apps/desktop/src/test/a11y.test.tsx` runs axe on Home, every settings page, a repository page and the
  welcome dialog. Colour contrast is covered separately by `pnpm check:contrast`, because jsdom
  cannot measure it.
- `prefers-reduced-motion` turns animations and transitions off (`globals.css`).
- **Manual:** the keyboard-only pass through Home, Create, a workspace and Settings.

## Known gaps

- No code signing, notarization or auto-update until an Apple Developer account exists.
- Universal binary needs `rustup` with both macOS targets. The machine used for development has
  Homebrew Rust with the arm64 target only.

## Composers (C7)

| Item | Result |
|---|---|
| The app decides, not the model | Verified by tests in `composer/dispatch.rs`: a tool set to ask always asks, whatever text the model read. There is no tool that changes a Composer's settings, approves, pauses or runs a shell. |
| Outside text | Results that carry text from pull requests, files or agents are wrapped in `<untrusted-data>` and the tag name is changed wherever it appears inside, so text cannot close the label early. Messages that start triggered runs do the same. |
| Tool server | A Unix socket, mode `0600`, in a `0700` folder. Every request needs a token made at app start (compared in constant time). The helper reads it from a `0600` file. |
| Audit | Every call is recorded, allowed or not, with redacted inputs and redacted results. The page can search and export CSV. |
| Limits | Steps, agents at once and in total, minutes (checked in the app), and a spending limit (Claude Code's own). Daily cap on runs started by triggers. |
| Kill switch | Stop all stops every Composer run and every agent a Composer started, denies waiting approvals and pauses Composers. Button, command palette, Cmd+Shift+. and the menu bar item. |
| Crash | A call waiting for approval is marked expired when its run goes away or the app restarts. A call that ran is never repeated by the app. **Manual:** force quit during a run with an approval waiting, relaunch, and check the run resumes and nothing happened twice. |
| Not done | Real tools on a real repository by a real Composer (see `composers/status.md`), performance with many runs, keyboard-only pass of the new screens by hand. |

## Jira (integrations)

- The API token lives only in the macOS Keychain. It is not in `settings.toml`, the database, an env
  var, the MCP config file, a log or the audit log. Tests check that the token is not printed by
  `Debug`, not in audit rows and not left on the screen after saving.
- The Jira address is fixed when connecting and limited to `*.atlassian.net`. No tool takes a URL.
- Issue keys, project keys, transition ids and account ids are validated before they are put in a path.
- Reads are labelled as untrusted data. Everything that changes Jira is External tier.
- A chat's Jira door reaches only the Jira tools.
- Sign in with Atlassian: PKCE (S256) and a random `state`; the browser returns to a listener on
  127.0.0.1 only, which serves one callback at a time, ignores any other path or wrong `state`, and
  stops after five minutes or Cancel. Authorization and token addresses must be HTTPS. The scopes
  never include delete or manage. Access and refresh tokens are only in the Keychain, and not in
  logs or `Debug`. Tools of Atlassian that Circles has no mapping for cannot be reached.

## Smart titles

- The model runs inside the Circles process on the CPU. There is no server, no port and no network
  call. A different provider (Claude Haiku) is a setting that is off unless chosen, and says in the
  settings that it sends text to Anthropic.
- Text given to a model is cut (1,500 characters for a title, 600 and 900 for a suggestion) and,
  for suggestions, goes through the redactor first.
- A title is plain text. Quotes, markdown marks, labels, questions, replies and greetings are
  removed or refused, and a title is at most 6 words and 50 characters.
- A title made by a model never replaces a name a person gave: `sessions.title_source` is `user`
  for every chat that existed, and for every rename.
- A missing, corrupt or slow model changes nothing for the chat: the first words stay as the title.
- The model is loaded on first use and dropped after five idle minutes.

## Side questions

- A side question is a separate, short Claude Code process on a fork of the chat. It is started with
  `--tools ""` (or only Read, Grep and Glob when the setting allows), `--strict-mcp-config` with an
  empty server list, `--settings {"disableAllHooks":true}`, `--no-session-persistence` and no
  permission bypass. It cannot change a file, run a command, reach a connected service, run a hook
  or leave a session behind.
- The question passes through the redactor. The answer is cut at 8,000 characters. There is a
  60 second limit and a Stop button, and the process is killed with the task.
- One side question runs per chat. A new one stops the old one, and a late ending of the old one
  does not forget the new one.

## Cards from agents (generative UI)

- An agent can only ask for the components in the catalogue. Everything in a card is plain text
  and is drawn as text. There is no HTML, no script, no style and no image from the web.
- A card is limited (30 components, 32 KB, 100 options, 50 table rows) and checked before it is
  stored. A bad card is an error the agent can read, and nothing is drawn.
- A form field that looks like a secret (password, token, secret, key) is refused.
- An answer is checked against the card: a value the card did not offer, a missing required
  field, or something the card did not ask for is refused. A card is answered once.
- A button returns its value. It never runs a tool. The agent then acts under the usual
  permissions.
- At most 3 waiting cards per chat and one new card a second. A waiting card expires when the
  app restarts or the call goes away.
- Text from outside that an agent puts in a card is marked "Includes text from outside".
