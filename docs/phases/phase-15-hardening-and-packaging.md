# Phase 15: Hardening and packaging

## Goal
Make Circles safe, stable, and installable.

## Depends on
All previous phases.

## In scope
- **First-run flow:** welcome, dependency check (`git`, `gh`, `claude` found and authenticated), security notice that agents run with the user's permissions, add first project.
- **Crash and recovery audit:** kill the app at every lifecycle point (creating, setup, working, blocked, archiving) and verify relaunch reconciles state without data loss or orphans.
- **Performance pass:** startup time, memory with 10 workspaces, long chat scrolling, big diffs, terminal throughput.
- **Security pass:** path traversal checks, command injection review (all spawns use argument arrays, never shell strings from user input), Tauri capability and CSP review, secret redaction in logs, deep-link input validation.
- **Error UX:** consistent toast and inline error patterns, "copy diagnostics" button, export logs.
- **Accessibility:** keyboard-only navigation across all screens, focus order, reduced-motion support, ARIA on custom components.
- **Test suite:** Rust unit and integration tests in CI-less local script, frontend component tests for key flows, a Playwright-style smoke test for the main path if feasible.
- **Packaging:** app icon, `.dmg` build, universal binary (arm64 and x86_64), versioning scheme, changelog.
- **Code signing and notarization** and **auto-update** (Tauri updater) once an Apple Developer account is available.
- **Docs:** user README, troubleshooting guide, contributing notes.

## Out of scope
Windows and Linux builds, cloud features.

## Deliverables
- Signed (or unsigned, if no account yet) `.dmg`, release checklist, test scripts, audit notes in `docs/`.

## Technical notes
- Treat the audit as a checklist and record results in `docs/hardening.md`.
- Never ship with the bypass-permissions mode on by default.
- Updater keys must be generated and stored outside the repo.

## Done checklist
- [ ] Fresh machine install reaches a working workspace using the first-run flow
- [ ] Crash audit passes at every lifecycle point
- [ ] No orphan processes or worktrees after any failure
- [ ] Security review items closed or documented
- [ ] Keyboard-only run through all screens works
- [ ] `.dmg` installs and launches on a clean user account
- [ ] Release checklist completed

## Manual test script
1. On a clean macOS user, install the `.dmg`, run the first-run flow.
2. Create five workspaces with running agents, force quit, relaunch, verify recovery.
3. Run the keyboard-only pass through Home, Create, a workspace, and Settings.
4. Grep logs and DB for secrets.
5. Run the full test suite, confirm green.
