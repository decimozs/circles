# Phase C2: Composer v1 on Claude

> **Built.** What it delivered, and what is still open, is in [status.md](status.md).

## Goal
A working Composer you can talk to, driven by Claude Code, that uses the registry through MCP.

## Depends on
C1.

## In scope
- **Composer definition:** name, instructions, allowed tools, allowed projects, limits. Stored in the DB.
- **MCP adapter:** the Circles binary has a tool-server mode. Claude Code starts it with `--mcp-config`. It forwards each tool call to the running app over a private socket (mode `0600`, per-run token) and returns the result. It exposes only the tools the Composer allows.
- **Run engine:** starts a Claude Code process for the Composer, in a scratch folder with no repository, with no file tools and only the Circles tools. Stores every step.
- **Run log and resume:** steps are written as they happen. After a restart a run shows as interrupted, and the person can resume it. A step that was mid-call is reconciled from the audit log, never repeated blindly.
- **Autonomy:** each tool of a Composer is `ask`, `allow` or `off`, with safe defaults. A tool that asks shows an approval card, which reuses the permission card. The Details tab shows the Composer's autonomy at a glance and warns when an `external` or `destructive` tool is allowed.
- **UI:** as described in `ui.md`: a Composers nav item below Create, the Composers home, a Composers group in the sidebar laid out like projects (runs listed under each Composer, pinning supported), the run page with the chat and a stop button, the Steps and Details tabs in the right pane, and a create and edit dialog. Routes `/composers`, `/composer/:id`, `/composer/:id/run/:runId`.
- **Scope of tools:** read tools plus `create_workspace`, `start_chat`, `send_message` and project creation (`add_local_project`, `clone_github_project`, quick start), so a Composer can make new projects and also create workspaces in existing ones.

## Design rule for later
Build runs so they never need a person in the window: a tool that asks must be able to wait (stored, shown in the UI, answerable later), and a run must be able to start from code and not only from the chat box. C6 depends on this.

## Out of scope
Watching children over time (C3), other models (C4), templates, schedules.

## Deliverables
- `apps/desktop/src-tauri/src/composer/`, the tool-server mode, `apps/desktop/src/features/composers/`.
- Commands: `composer_create`, `composer_update`, `composer_list`, `run_start`, `run_stop`, `run_resume`, `run_get`, `approval_reply`.

## Technical notes
- The Composer's Claude process runs with the permission-prompt tool, so a tool it should not use never runs without a decision.
- The socket is removed when the run ends, and the token is never written to disk or logs.
- A fake Claude (like the existing one) drives the tests. Only opt-in tests use the real one.

## Done checklist
- [ ] A Composer can list projects and start a workspace and a chat in it
- [ ] The child agent starts and its first message is the task given
- [ ] A run survives an app restart and resumes without repeating a step
- [ ] A `destructive` tool asks by default, and does not once the person allows it for that Composer
- [ ] Stopping a run stops its Claude process and its socket
- [ ] No token or secret appears in the log or the database

## Manual test script
1. Create a Composer that may use the read tools and `create_workspace`.
2. Ask it to start a workspace in a project and give the agent a task.
3. Force quit during a run, relaunch, resume, and check nothing was done twice.
4. Ask it to delete a workspace and confirm the approval card appears. Allow the tool and ask again to confirm it goes ahead.
5. Ask it to create a new project and then a workspace in it.
