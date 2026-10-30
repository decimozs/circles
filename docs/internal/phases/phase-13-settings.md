# Phase 13: Settings pages

## Goal
The full settings area, reading and writing `settings.toml` at the right scope.

## Depends on
Phases 2 and 3. Most pages need features from phases 7-11 to be meaningful.

## In scope
- **Layout:** Back button, settings search (Cmd+F), grouped nav, "Open settings.toml" button with a dropdown (user, project, local).
- **Reusable `SettingRow`** (title, description, control right) bound to a key and scope.
- **Personal**
  - General: send key (Enter or Cmd+Enter), follow-up behavior (Steer or Queue), agent notifications, completion sound (with test), full disk access shortcut, auto-convert long text.
  - Account: placeholder until phase 14.
  - Appearance: theme (light, dark, system), font sizes, sidebar density.
  - Default models: model and effort defaults, plan or fast defaults.
  - Git: branch prefix, auto-rename toggle, default merge strategy, author info display.
- **Agents and environment**
  - Agents: Claude Code path and version check, credentials guidance, permission mode default, "bypass" warning, extra CLI args.
  - Environment: user-level env vars (values masked), MCP servers (add, edit, enable per project).
- **Repositories** (one entry per project): base branch, scripts (setup, run, archive, run mode), files to copy, `.worktreeinclude`, instruction presets (general, review, PR, fix errors, conflicts, branch rename), merge method, "share settings" (write `.circles/settings.toml` into the repo), remove project.
- **More**
  - Experimental: feature flags.
  - Advanced: data paths, reveal data folder, log level, export logs, reset app data (with confirmation), concurrency soft cap.
- **Notifications** via the native macOS API: agent finished, agent blocked. Click opens the workspace.

## Out of scope
Account and sign-in (phase 14), managed or org settings.

## Deliverables
- `apps/desktop/src/features/settings/*` with one file per page, shared `SettingRow` and search index.
- Backend commands: `settings_get`, `settings_set`, `settings_open_file`, notification and sound helpers.

## Technical notes
- Writes preserve comments and formatting in the TOML (use `toml_edit`).
- Validate on save, show inline errors, never write a broken file.
- Watch the settings files and reflect external edits live.
- Mask secret values in the UI, and never log them.

## Done checklist
- [ ] Every setting reads and writes to the correct scope
- [ ] External edits to `settings.toml` appear live
- [ ] Comments in the TOML survive a save
- [ ] Settings search finds rows by title and description
- [ ] Notifications and sound fire and open the right workspace
- [ ] Reset app data requires explicit confirmation
- [ ] Both themes look correct on every page

## Manual test script
1. Change a user setting, open the file, confirm the key and that comments remain.
2. Edit the file in an editor, confirm the UI updates.
3. Set a repo-level override, confirm it wins in that project only.
4. Finish an agent turn with the app in the background, confirm the notification and click-through.
5. Search for "sound", confirm the row is found and highlighted.
