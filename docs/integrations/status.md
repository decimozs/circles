# Jira integration: status

## Sign in with Atlassian (J7)

Built: **Connect Jira** now opens the browser (OAuth 2.1 with PKCE through Atlassian's hosted MCP
server) and the API-token form is behind "Use an API token instead". Signing in registers Circles
as a public client, asks for Jira read, write and search plus offline access (never delete or
manage), and keeps the tokens in the Keychain (`circles` / `jira-oauth`). A one-shot listener on
`127.0.0.1` receives the browser's return, checks `state`, and ignores anything else. Access tokens
are renewed before they expire. If the sign-in cannot be renewed, the card says to sign in again.
An account with several Jira sites gets a picker. The ten `jira_*` tools are the same for agents:
they are answered through Atlassian's tools, each argument checked against the schema the server
gives, so a tool that Atlassian changed fails with a plain message ("use an API token instead").
Only the mapped tools are reachable.

Not yet done: **the real sign-in has not been seen working.** The spike (J7.0) opened the browser
and nobody approved it. Registration with a `localhost` redirect and the scope names were confirmed
against the real server. Everything else is tested against a fake Atlassian. The mapped Atlassian
tool names and argument names come from documentation and may need a small fix once the real
tool list is seen. See `phase-j7-atlassian-signin.md` for the command.

Other J7 gaps:
- Your organisation can block the redirect or the app. Then the sign-in fails and the token form is
  the fallback.
- Calls use the organisation's Rovo credits, and the server is documented as beta.
- `jira_get_create_fields`, `jira_assign_issue` and the comment tools depend most on exact
  Atlassian argument names. Test them first.
- Disconnect removes the saved sign-in but does not revoke it at Atlassian. Revoke it at
  id.atlassian.com, under connected apps, if you want it gone there too.

All phases J1 to J6 are built. Nothing has been run against a real Jira site yet. The manual guide
below is what is left.

## What is built

- **Connection (J1):** `integrations/` in Rust. Site, email and API token are checked against
  `GET /rest/api/3/myself`, then saved in the macOS Keychain (service `circles`, account `jira`).
  Only `name.atlassian.net` sites are accepted, the address is fixed at connect time, redirects are
  off, answers are capped at 4 MB. A 401 or 403 later marks the connection as needing a reconnect.
- **Settings (J2):** Settings, Integrations holds only the connection: the Jira card with the Jira mark
  (`SiJira`, `--brand-jira`), Connect, Reconnect (site and email filled in, token empty) and
  Disconnect (asks first). Once Jira is connected the sidebar gets an "Integrations" section with a
  "Jira" page: who it is connected as and the switch "Let chats use Jira"
  (`integrations.jira_in_chats`, on by default). New services add their own page there.
- **Tools (J3):** ten, in `integrations/jira/tools.rs`. Read: `jira_search`, `jira_get_issue`,
  `jira_list_projects`, `jira_get_transitions`, `jira_get_create_fields`. External tier (asks, even when edits are accepted):
  `jira_add_comment`, `jira_transition_issue`, `jira_assign_issue`, `jira_edit_issue`,
  `jira_create_issue`. No delete. Keys and ids are validated before they reach an address. Results of
  reads are labelled `<untrusted-data>`.
- **Composer dialog (J4):** Repositories (all, or only the picked ones, Create disabled with none
  picked) and Integrations (Jira switch, disabled until Jira is connected). `composers.integrations`
  is migration 0011. Shared `.circles/composers/*.toml` files cannot switch Jira on. Disconnecting
  switches Jira off on every Composer.
- **Chats (J5):** a chat that starts while Jira is connected and allowed gets a second tool server,
  `circles-jira`, next to the user's own (not strict). Reads are in `--allowedTools`. Writes are not,
  so Claude's own permission prompt asks in the chat, and bypass lets them run. The helper can reach
  only the Jira tools for such a chat, never the workspace, project or merge tools. Audit rows carry
  the session and no Composer.
- **Trigger (J6):** `jira_issue`, a JQL query polled like the GitHub triggers: baseline first,
  dedupe by issue and update time, daily cap, five failures switch it off. Skipped without a failure
  when the Composer has Jira off or Jira is not connected. Text from Jira is wrapped as untrusted.

## Known gaps

- Never run against a real Jira Cloud site. Atlassian's answers could differ from the fake in small
  ways (field names on unusual issue types, custom workflows, required fields on create).
- Required fields: `jira_get_create_fields` lists what a project and issue type ask for, and `jira_create_issue` takes them in `fields`. Only fields Jira lists are accepted. Values for unusual field kinds (user pickers, cascading selects) are passed as given, so the shape Jira expects may need a retry.
- Chats that were already running when Jira was connected do not get it. Start a new chat.
- A chat's Jira calls are in the audit log (session, no Composer) but the audit page lists by
  Composer, so they do not show there yet.
- No system line tells a chat that Jira text is data. The results are labelled, and the tool
  descriptions say so.
- The Keychain was not exercised: tests use an in-memory store. macOS may ask for access the first time.
- API tokens carry all the permissions of the account. The Connect dialog says so.

## Manual guide (real Jira Cloud)

1. Settings, Integrations, Connect. First a wrong token: the dialog stays open with a plain message.
2. Then a right one. The card says "Connected as <name> on <site>". Check Keychain Access for `circles`.
3. New Composer: Jira switch is on-able. Pick "Only the repositories I pick" with one repository.
4. Ask the Composer: "search Jira for my open issues" (runs without asking), then "comment on PROJ-1"
   (asks first; decline once, then allow). Check the comment in Jira.
5. A Composer with Jira off: it has no jira_* tools.
6. A normal workspace chat, started after connecting: "what is PROJ-1 about". It reads without a
   prompt. "Move it to Done": Claude's permission prompt appears.
7. Switch "Let chats use Jira" off, start a new chat: no Jira tools.
8. Add a Jira trigger with `project = PROJ AND updated >= -1d`. Wait a poll. The first look starts
   nothing. Edit an issue: a run starts.
9. Revoke the token in Atlassian. The next Jira call says to reconnect and the card says so.
10. Disconnect: the Keychain item is gone and the Composers have Jira off.

## Manual guide, sign in with Atlassian

1. Settings, Integrations, Connect, Sign in with Atlassian. The browser opens Atlassian. Approve.
   Circles says "Connected as <you> on <site>, signed in with Atlassian".
2. Close the tab without approving: after five minutes, or Cancel, the dialog ends with a message.
3. An account with two sites: the picker appears. A wrong pick can be corrected.
4. Run the manual guide above (search, read, comment, chats, trigger). Everything should work the same.
5. Revoke the app at id.atlassian.com: the next call says to sign in again, and the card says so.
6. Block it as an admin if you can: Connect shows why, and "Use an API token instead" still works.
