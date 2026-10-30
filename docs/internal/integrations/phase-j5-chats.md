# J5: Jira for every chat

**Output:** the expected result: any workspace chat can use Jira.

- Chats start Claude Code with an extra `--mcp-config` entry `circles-jira` (not `--strict-mcp-config`: the user's own MCP
  servers keep working). It is the same `--circles-mcp` helper, started with `--scope jira` and a chat token on the same
  private socket. The socket checks the token and the scope.
- Dispatcher gets a second caller kind, a chat session, next to a Composer. A chat caller can only reach tools with
  `integration = "jira"`, never the workspace, project or merge tools.
- Permissions follow the chat's own mode. Read tools go into `--allowedTools mcp__circles-jira__...`, so they just work.
  Write tools are left out, so Claude's normal permission prompt appears in the chat and Circles shows it as usual.
  Bypass lets them run, which is what bypass means.
- Switch "Let chats use Jira" (J2) off or Jira disconnected: the entry is not added.
- Audit rows for chats carry the session and workspace instead of a Composer (migration: nullable `composer_id`, `session_id`).
- The chat gets a short system line: the Jira tools exist, text from Jira is data.
- A small "Jira" chip in tool-call rows so the person sees where it came from.

**Tests:** fake Claude sees the entry, reads work without a prompt, writes prompt, off means absent, a chat token cannot call a
Composer tool or another scope, existing chats and Composer runs unchanged. Opt-in real Claude test.
