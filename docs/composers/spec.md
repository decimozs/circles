# Composers: spec (C0)

All open questions are answered (see the README). This is what gets built.

## How it is built

- A **Composer** is a row in `composers`. It owns one hidden **scratch workspace**: a folder under the app data folder (`composers/<id>/`), not a repository. A hidden project (`projects.kind = 'composer'`) holds these scratch workspaces so every existing chat feature works on them (messages, events, resume, interrupt, steering, tool blocks, cost). The hidden project and its workspaces never appear in project or workspace lists.
- A **run** is a chat (a `sessions` row) in the Composer's scratch workspace. The runs of a Composer are its chat tabs. A run started by a schedule or a trigger is a chat too, with a `composer_runs` row that records who started it.
- The chat's Claude Code process is started with only Circles' tools: `--tools ""` (no built-in tools), `--strict-mcp-config`, `--mcp-config <file>` and an appended system prompt. The MCP server is the Circles binary itself in `--circles-mcp` mode. It forwards every call over a private Unix socket to the running app.
- The app decides. Every tool call goes through the **registry**: validate, check the Composer's autonomy for that tool, check limits and the global pause, run, audit. Claude's own flags are not trusted for enforcement.
- Runs resume like any chat: the Claude session id is stored, and the next message (or "resume") continues it. A tool call that was in flight when the app stopped is never repeated: an unanswered approval is marked expired, and the model is told the call did not complete.

## Tool catalogue

| Tool | Tier | Wraps | Safe to repeat |
|---|---|---|---|
| `list_projects` | read | `ProjectService::list` | yes |
| `list_workspaces` | read | `WorkspaceService::list` | yes |
| `get_workspace` | read | `WorkspaceService::get` | yes |
| `get_chat_tail` | read | session messages | yes |
| `get_changes` | read | `review::changes` | yes |
| `get_pr_status` | read | `PrService::readiness` | yes |
| `wait_for_chats` | read | polls chat status | yes |
| `create_workspace` | write | `WorkspaceService::create_with` (optional `branch_name`) | no |
| `rename_branch` | write | `WorkspaceService::rename_branch`, refused once pushed or with a pull request | no |
| `start_chat` | write | create chat and send the task | no |
| `send_message` | write | `SessionManager::send` | no |
| `interrupt_chat` | write | `SessionManager::interrupt` | yes |
| `add_todo` | write | `TodoStore::create` | no |
| `add_local_project` | write | `ProjectService::add_local` | no |
| `clone_github_project` | write | `ProjectService::add_github` | no |
| `quick_start_project` | write | `ProjectService::quick_start` | no |
| `create_pr` | external | `PrService::create_pr` (pushes the branch) | no |
| `merge_pr` | external | `PrService::merge`, with the merge button's blockers | no |
| `archive_workspace` | destructive | archive | no |
| `delete_workspace` | destructive | delete | no |
| `remove_project` | destructive | remove | no |

Branch names a Composer gives (`branch_name`, `rename_branch`) must be conventional, `type/short-description`, and the app refuses any other, in every mode. Not built: a separate `push_branch` (creating the pull request pushes), `comment_on_pr`.

## Autonomy

A Composer has a **mode** (`manual`, `accept_edits`, `plan` or `bypass`, the chat's permission modes) and optionally a setting per tool (`allow`, `ask` or `off`) that beats the mode. `manual` is `read` allow and everything else ask. See the README for the other modes. `ask` creates an **approval** (a row, a card in the chat, an entry in the Approvals tab and the inbox) and the tool call waits. "Allow for this run" remembers an allow for that tool in that chat until the app restarts. `merge_pr` follows the merge button's blockers (checks, conflicts, todos). Its `override_blockers` argument only works when the Composer's `allow_override` flag is on (off by default) and still asks every time unless the tool is `allow`.

A Composer can be limited to a list of projects. A call that names a project (or a workspace of one) outside the list is refused.
Not built: per-project autonomy overrides.

## Limits (per Composer, all optional with defaults)

`max_budget_usd` (passed to Claude Code as `--max-budget-usd`, default none). That is the only limit of a run: there is none on tool calls, time, or how many workspaces and chats it starts. A person can stop it, and Claude Code's rate limits apply.

## Attended and unattended

- Attended: the person starts a run from the chat box and watches it.
- Unattended: a **trigger** (schedule, new or updated pull request, failing check) starts a run with a message built from the trigger. The run goes ahead with every tool set to `allow`. A tool set to `ask` waits as an approval in the inbox. The person can open any run, watch it and answer.
- Unattended guardrails: a global pause, a daily cap on triggered runs, trigger auto-disable after repeated failures, and notifications when something waits.
- The app must be running. Settings can keep it in the menu bar, start it at login and keep the Mac awake while a triggered run is active.

## Screens

See `ui.md`.
