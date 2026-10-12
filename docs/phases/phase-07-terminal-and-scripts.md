# Phase 7: Terminal and scripts

## Goal
A real terminal per workspace, plus setup, run, and archive scripts with per-workspace environment and ports.

## Depends on
Phase 5.

## In scope
- **PTY service** (`portable-pty`): spawn login shell in the worktree with the resolved env, resize, write, kill. One PTY per terminal tab.
- **Streaming:** PTY bytes to the frontend over a Tauri `Channel`, batched at about 16ms to avoid flooding.
- **xterm.js** frontend with fit, search, and web-links addons. Theme tokens map to light and dark.
- **Right-panel terminal dock:** collapsible, tabs Setup, Run, Terminal, `+` for more terminals. Big Terminal Mode (full terminal in the center).
- **Scrollback persistence** (last N KB) and the "previous terminal session restored" banner.
- **Scripts** from `.circles/settings.toml`: `scripts.setup`, `scripts.run` (multiple named, each with `command`, `default`, `icon`, `available_in`), `scripts.archive`, `run_mode` (`concurrent` or `nonconcurrent`).
- **Run button** with dropdown and Cmd+R. Empty state "Start Dev".
- **Environment variables:** `CIRCLES_WORKSPACE_NAME`, `CIRCLES_WORKSPACE_PATH`, `CIRCLES_ROOT_PATH`, `CIRCLES_DEFAULT_BRANCH`, `CIRCLES_PORT`, plus custom `[environment_variables]`.
- **Port registry:** block of 10 ports from 55000, verified free before assignment, released on archive.
- **Process handling:** stop sends SIGTERM to the process group, SIGKILL after 5s. Kill all on quit (with confirm if anything runs).
- Setup script runs after workspace creation (moves workspace `setup_running` to `ready`). Archive script runs before archive.

## Out of scope
Agents, Spotlight testing (can be added later).

## Deliverables
- `apps/desktop/src-tauri/src/pty/`, `apps/desktop/src-tauri/src/scripts/`, `apps/desktop/src/features/terminal`.
- Commands: `pty_open`, `pty_write`, `pty_resize`, `pty_close`, `script_run`, `script_stop`.

## Technical notes
- Spawn in a new process group so group kill works.
- Record pids in the DB so a crash leaves nothing orphaned: reap on next launch.
- Cap scrollback and per-terminal memory. Drop output when the frontend is detached, not the process.
- Script output goes to its tab but is also kept in a log file per workspace.

## Done checklist
- [ ] Terminal opens in the worktree with correct env and PATH
- [ ] Resize, copy, paste, search, and colors work
- [ ] Setup runs after create, and state changes reflect it
- [ ] Two workspaces run dev servers on different ports at once
- [ ] Stop kills the whole process tree
- [ ] Quit with a running script prompts, and nothing is orphaned after
- [ ] Crash then relaunch reaps orphans

## Manual test script
1. Create a workspace with a setup script that echoes the env, watch it in the Setup tab.
2. Start the run script in two workspaces, confirm distinct `CIRCLES_PORT`.
3. Stop one, confirm the process tree is gone (`ps`).
4. `kill -9` the app, relaunch, confirm orphans are reaped.
5. Run `vim` and `htop` in the terminal, confirm they render and resize.
