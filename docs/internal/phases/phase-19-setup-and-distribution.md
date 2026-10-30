# Phase 19: First-run setup and distribution

> Changed later: the title model is no longer packed into the app, so `pnpm model:fetch`, `scripts/check-model.sh` and the "Built-in model" row of the setup checklist are gone.

Status: P0 (written audit), P1, P2, P3 and the unsigned parts of P4 and P5 are built. Signing, notarisation, the universal build and the updater are not. See "Built" and "Gaps" at the end.

## Goal

Someone who installs Circles for the first time is walked through a **Setup** screen with a
checklist: what the app needs, what it would like, and every macOS permission it may ask for, each
with a status and a button that does or explains the next step. When it is done the app works,
and nothing surprising appears later in the middle of a task.

The package they install is **signed, notarised, universal and updatable**, and carries what it
needs: the app, the title model, and no download at first launch.

## What exists today

- `WelcomeDialog` (`features/onboarding`): checks `git`, `gh` and `claude` (found, version, signed
  in), a security notice that agents run with your permissions, and "add your first project".
  It is shown on the first launch with no projects, and again from Settings.
- `dependency_check` and `diagnostics_report` commands, a release checklist
  (`docs/release-checklist.md`), `scripts/release.sh`, a `.dmg` build, the title model bundled
  as a resource with `pnpm model:check`.
- Not there: permission checks, a notifications permission flow, signing and notarisation, an
  updater, a universal build, a clean-account test.

## The checklist

Each row has a state (`ready`, `needs you`, `optional`, `skipped`), a one line reason, and an action.
Required rows block "Get started" until they are `ready`. Everything else can be skipped, and
comes back under Settings, Setup.

| # | Item | Required | How it is checked | Action |
|---|---|---|---|---|
| 1 | **git** (Xcode command line tools) | yes | `git --version` | "Install" runs `xcode-select --install` and re-checks |
| 2 | **Claude Code installed** | yes | `claude --version` | shows the install command, copy button, link to its page |
| 3 | **Claude Code signed in** | yes | `claude auth status`, or a tiny no-op call | "Open Terminal" with `claude`, then "Check again" |
| 4 | **GitHub CLI** and signed in | no | `gh auth status` (every account) | install and `gh auth login` commands |
| 5 | **Notifications** | no | the notification permission state | "Allow", then a test notification |
| 6 | **Folders** where your repositories live | no | try to read Desktop, Documents, Downloads, a chosen folder | "Allow access" does the read, so macOS asks now and not in the middle of a task |
| 7 | **Full Disk Access** | no, and not recommended by default | try a protected path | explains the trade-off, opens the right System Settings pane |
| 8 | **Keychain** | no | nothing until a service is connected | one line saying Jira tokens go there, and macOS may ask, click Always Allow |
| 9 | **Open at login and the menu bar** | no | the autostart and menu bar settings | two switches, for unattended Composers |
| 10 | **Built-in model** (titles, examples) | no | the file is there and loads, shows the time | "Test" |
| 11 | **Agents run as you** notice | yes | accepted once | the checkbox that exists today |
| 12 | **First project** | no | any project exists | Add project, or Clone from GitHub |

A footer shows "Copy diagnostics" for a bug report, and "Skip for now".

## What macOS may ask, and the plan for each

- **Files and folders (TCC):** the first read of Desktop, Documents, Downloads or a removable
  volume prompts. Row 6 triggers those prompts on purpose, during setup, with an explanation.
- **Notifications:** today notifications go through `osascript`, so they show as coming from
  Script Editor and cannot be allowed or denied for Circles. The plan is to move them to the native
  notification API through Tauri's notification plugin, so Circles gets its own permission entry
  and a test button. This needs the signed app.
- **Keychain:** a prompt appears per binary signature. A signed, stable build makes "Always Allow"
  last across updates, which the unsigned dev build cannot do.
- **Automation, Accessibility, Camera, Microphone, Location:** not used. The checklist says so.
- **Network:** nothing to allow. The checklist can show reachability of the services in use.

## Distribution

- **Signing:** a Developer ID Application certificate, hardened runtime, an entitlements file
  (child processes and the bundled model need none of the unsafe ones, which the audit confirms).
- **Notarisation:** `notarytool` and stapling in the release script, so Gatekeeper opens it
  without a warning.
- **Universal build:** arm64 and x86_64, including llama.cpp for both. Needs `rustup` with both
  targets, and a measured build time.
- **Updater:** the Tauri updater with a signing key kept outside the repository, a release feed,
  and an in-app "Update available" with release notes. The 270 MB model makes full updates large:
  see the decision below.
- **Pipeline:** a macOS runner builds, signs, notarises, uploads the `.dmg` and the update feed, and
  runs the model check and the test suites first.
- **A first-launch guide** for an unsigned build (right-click, Open), kept in
  `docs/troubleshooting.md`, for as long as there is no certificate.
- **Uninstall and reset:** documented paths (`~/Library/Application Support/circles`, the Keychain
  item, the login item), and the existing "Reset" in Settings.

## Phases

| Phase | What | Output |
|---|---|---|
| P0 | **Audit.** On a clean macOS user account (and with `tccutil reset All` for the app), run the app and write down every prompt, every missing tool and every failure. Also try an unsigned and a signed build | `docs/setup-audit.md` |
| P1 | **Backend:** `setup_status` command that returns the rows (state, reason, action), one command per action (install git, open Terminal, request notifications, probe a folder, test the model), tests with fakes for every tool and permission state | Rust |
| P2 | **The screen:** a full-window Setup with the checklist, live re-checks, keyboard use, both themes, "Skip", "Run setup again" in Settings. It replaces `WelcomeDialog` and keeps its notice and first project steps | UI |
| P3 | **Notifications:** move to the native API with a permission request, a test notification, and a fallback to `osascript` when it is unavailable | Rust and UI |
| P4 | **Packaging:** entitlements, signing, notarisation, universal build, `release.sh` updated, the updater and its key handling, update UI | `.dmg` that opens cleanly |
| P5 | **Release pipeline and docs:** CI workflow, release checklist, troubleshooting, a clean-account test run recorded | done |

Order: P0 first. P1 and P3 can go ahead of the Apple account. P4 needs the account.

## Risks

- **No Apple Developer account yet:** signing, notarisation, a persistent Keychain "Always Allow"
  and the native notification identity all depend on it. P0 to P3 are useful without it.
- **A prompt cannot be undone from the app.** If someone denies a permission, the checklist
  shows how to fix it in System Settings and checks again, but it cannot do it for them.
- **`claude auth status` may change.** The check is isolated, and falls back to "could not tell".
- **Update size** (below).
- **Detecting a permission is not always possible.** Some are only learned by trying, so a row
  can say "not asked yet" instead of "denied".

## Decisions for you

1. **Do you have, or will you get, an Apple Developer account** (99 USD a year)? Without one the
   package is unsigned and every user has to right-click, Open.
2. **Where do updates come from?** GitHub Releases (free, simple) or your own host.
3. **The 270 MB model and updates:** ship it inside every update (simple, large), or keep it in
   `~/Library/Application Support/circles/models/`, copied from the app once and only replaced when
   the model changes. The second makes updates small, and still needs no download for a new user.
   Recommended: the second.
4. **Block on Claude Code sign-in** (recommended), or let people in without it and show the
   problem in the app?
5. **Notifications:** move to the native API (recommended, needs the signed build) or keep
   `osascript`?

## Decisions taken

1. **No Apple Developer account for now.** The app is downloaded from the web, unsigned, and the
   README and troubleshooting say right-click, Open (and the `xattr` command).
2. **Updates:** GitHub Releases, when there is a repository to publish to. Not built.
3. **The model:** kept in `~/Library/Application Support/circles/models/`, copied once from the
   app, so a later update that does not change the model does not have to bring it again.
4. **Claude Code sign-in blocks** the first project, as the plan recommended.
5. **Notifications:** sent by Circles itself, with a script as a fallback.

## Built (2026-10-11)

- **P0:** `docs/setup-audit.md`, from the code. The clean-account run itself is still to do.
- **P1:** `commands/setup.rs`: `setup_status` (rows with state, reason, fix and action) and
  `setup_action` (`install_git`, `open_terminal_claude`, `test_notification`, `probe_folders`,
  `open_full_disk`, `test_model`). Tested with every state of git, Claude Code and gh.
- **P2:** the Setup checklist replaces the welcome dialog: "What Circles needs" and "Nice to have",
  each row with a state, a reason, a command to copy and a button, re-checked when the window gets
  focus again, "Add your first project" held back until the required rows are ready, "Skip for
  now" and "Copy diagnostics" kept. Re-run from Settings, Advanced, Show again.
- **P3:** notifications go through the notification plugin, with `osascript` as the fallback.
- **P4 (unsigned parts):** the model copy in Application Support, the model check in the release
  script, the install notes for an unsigned app.
- **P5:** the README, troubleshooting and release checklist are updated.

## Gaps

- **Notification permission cannot be queried or asked for** on macOS through the plugin: on
  desktop it always reports "granted" and never prompts. So the row sends a test and the person
  says whether they saw it. A real permission flow needs `UNUserNotificationCenter` through
  native code and a signed app.
- **Not signed, not notarised.** Every first launch needs right-click, Open. The Keychain's
  "Always Allow" will not last across rebuilds, and an unsigned notification may not be shown as
  Circles on every macOS version.
- **No universal build.** This Mac has no second Rust target (`rustup` is missing), so Intel was
  not built. llama.cpp for Intel is untried.
- **No updater.** It needs a repository to publish to, a signing key kept outside the repo, and
  the feed. Updates are still a full download, 350 MB.
- **The bundle still carries the model.** The copy only saves a second download later.
- **No clean-account run.** The audit is from the code. A real run may find prompts not listed.
- **"Open Terminal" needs Automation permission** for Terminal on first use.
- **The Claude sign-in check** depends on what `claude` reports. If its wording changes the row
  may say "needs attention" for someone who is signed in. "Skip for now" is the way past it.
- **Not seen by eye** in the running app, in both themes.
