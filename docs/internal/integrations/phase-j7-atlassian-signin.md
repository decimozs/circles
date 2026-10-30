# J7: Sign in with Atlassian (OAuth through Atlassian's remote MCP server)

Status: J7.1 to J7.5 are built and tested against a fake Atlassian. **J7.0, the spike against the
real Atlassian, still needs you:** the sign-in opened the browser and waited five minutes, and nobody
approved it, so the real flow has not been seen working. See "What was learned" below, and the
manual guide in `status.md`.

## What was learned from the real server (2026-10-09)

Confirmed against Atlassian, without signing in:
- The protected-resource metadata names the authorization server, and its metadata has a
  registration endpoint. Dynamic registration **accepted** a public client with the redirect
  `http://localhost:<port>/callback` (no secret).
- The scopes are named `read:jira:agent-interface`, `write:jira:agent-interface`,
  `search:jira:agent-interface`, `delete:jira:agent-interface`, `manage:jira:agent-interface`, plus
  `offline_access`. Circles asks for the first three and `offline_access`, never delete or manage.
- Not confirmed: that the browser sign-in itself completes for your organisation, the real tool list
  and argument names (the mapping uses the names from Atlassian's documentation and checks them
  against the live schema at runtime), and `127.0.0.1` as a redirect name.

To finish J7.0 run, in `src-tauri`, and approve in the browser within five minutes:

```
SPIKE_HOST=localhost SPIKE_OUT=/tmp/atlassian-tools.json cargo test spike_real_atlassian -- --ignored --nocapture
```

It prints what it learns and writes the real tool schemas to `SPIKE_OUT`. Try
`SPIKE_HOST=127.0.0.1` too. If the mapped tool names or arguments differ, only
`integrations/atlassian/tools.rs` needs changing.


## Goal

Connect becomes "Sign in with Atlassian": the browser opens, you approve, you are back in Circles.
No site, email or token to paste. The API-token form stays as a fallback ("Use an API token
instead"). The ten Circles Jira tools keep their names, schemas, tiers and audit. Only where the
call goes changes.

## What the research found

- **Server:** `https://mcp.atlassian.com/v2/mcp` (v2, recommended). The old `/v1/sse` ends on
  30 June 2026, so it is not an option. Bearer token in the `Authorization` header.
  `?tools=all` gives a flat, paginated tool list instead of the default discover/execute model.
- **Sign-in:** OAuth 2.1 with PKCE (S256). The protected resource names an authorization server at
  `https://auth.atlassian.com/<id>` whose metadata lists a **dynamic client registration** endpoint
  (`.../dcr/register`), the `none` token auth method (public client, no secret) and `refresh_token`.
  So Circles can register itself. Do not use the metadata at `mcp.atlassian.com/.well-known/...`:
  it is the older one. Follow the MCP spec: protected-resource metadata, then authorization server
  metadata.
- **Redirect:** Atlassian says the server only allows `localhost` and partner domains, and org
  admins can allow or block domains for OAuth. A request to allow custom redirect URIs or your own
  OAuth app (ROVO-870) is open and unanswered. Reports exist of an org blocking `localhost` while
  `127.0.0.1` worked, and of the reverse. **Nobody can tell from outside whether Circles' redirect
  will be accepted for your organisation.** That is the main risk and why J7.0 is a spike.
- **Atlassian's own OAuth app (3LO)** needs a client secret at the token step and no PKCE-only
  option is documented. A desktop app cannot keep a secret. Not used.
- **Tools (v2):** primary tools (`getJiraIssue`, `createJiraIssue`, `editJiraIssue`,
  `transitionJiraIssue`, `addOrEditJiraIssueComment`, `searchJiraIssuesUsingJql`, ...) plus the rest
  behind `discover` and `executeRead/Write/Destructive`. Every call needs a `cloudId`, got from
  `getAccessibleAtlassianResources`. Permission groups: `read_jira`, `write_jira`, `search_jira`;
  `delete_jira` and `manage_jira` are off unless an admin turns them on. New tools can appear without
  reconnecting.
- **Cost and status:** calls use the organisation's Rovo credits (heavy Teamwork Graph tools much
  more than Jira reads). The server is documented as beta.
- **Admin control:** admins can see the app under third-party apps, revoke it, restrict by IP, and
  see "Rovo MCP User Actions" in the audit log. API-token use of the same server needs an admin to
  enable it, so it is not an easy route around the above.

## Design

Keep the Circles tool layer. Do **not** pass Atlassian's tools straight to agents.

```
Composer / chat  ->  Circles jira_* tool (tier, autonomy, audit, untrusted label)
                          |
                  JiraBackend (one trait)
                   |                      |
          RestBackend (today)     AtlassianMcpBackend (new)
                                   OAuth token in Keychain, Streamable HTTP
```

Why keep our own tools: Atlassian can add or change tools at any time. With our mapping, an unknown
tool is simply unreachable, tiers stay ours (every write is External, asks first), Composers'
autonomy settings and the trigger keep working, and the REST fallback shares the same names.

Mapping (`jira_*` to Atlassian tool, arguments translated, `cloudId` added):

| Circles | Atlassian |
|---|---|
| jira_search | searchJiraIssuesUsingJql |
| jira_get_issue | getJiraIssue |
| jira_list_projects | listJiraProjects |
| jira_get_transitions | listJiraIssueTransitions |
| jira_get_create_fields | getJiraIssueTypeMetaWithFields |
| jira_add_comment | addOrEditJiraIssueComment |
| jira_transition_issue | transitionJiraIssue |
| jira_assign_issue | editJiraIssue (assignee) |
| jira_edit_issue | editJiraIssue |
| jira_create_issue | createJiraIssue |

The tool list is read from the server (`tools/list` with `?tools=all`) and each mapped tool's
schema is checked at connect and at start-up. If Atlassian changed a tool, the Circles tool says
so plainly ("Atlassian changed jira_search; use the API token instead") instead of sending a
wrong call.

## Phases

### J7.0 Spike (before any design is locked)
A throwaway test binary, run once with your account:
1. Fetch the metadata, register a client with a loopback redirect, PKCE, browser sign-in, token.
2. `tools/list?tools=all`, then `getAccessibleAtlassianResources`, `searchJiraIssuesUsingJql`,
   `getJiraIssue`.
3. Try both `http://127.0.0.1:<port>/callback` and `http://localhost:<port>/callback`.
4. Save the real tool schemas to `docs/integrations/atlassian-tools.md`.

**Gate:** if your organisation blocks the redirect, stop and keep the token form. Nothing else
in J7 is worth building.

### J7.1 OAuth core (Rust)
`integrations/atlassian/oauth.rs`: discovery, dynamic registration (once, client id kept),
PKCE, one-shot loopback listener on `127.0.0.1` (random port, `state` check, 5 minute timeout,
closed after one request), code exchange, refresh before expiry, revoke on disconnect where
supported. Tokens and client id in the Keychain (`circles` / `jira-oauth`). Scopes: read and
write for Jira, and offline access. Never delete or manage.
Tests with a fake authorization server: good flow, wrong `state`, timeout, refresh, refresh
refused (needs reconnect), token never in logs or `Debug`.

### J7.2 MCP client
`integrations/atlassian/mcp.rs`: Streamable HTTP (JSON or `text/event-stream` replies),
`initialize`, `Mcp-Session-Id`, `tools/list` with pagination, `tools/call`. Timeouts, a size cap,
one retry on a dropped session, 429 handling. Tests with a fake server.

### J7.3 Backend trait and mapping
Extract `JiraBackend` from today's `jira/tools.rs`. `RestBackend` is the current code. The new
`AtlassianMcpBackend` follows the table. Results are normalised to the shapes the Circles tools
return today so Composers and the trigger do not change. Everything still ends up wrapped as
untrusted. `Integrations` picks the backend by how Jira was connected.
Tests: the same tool tests run against both backends, using fakes.

### J7.4 UI
Connect dialog: a main "Sign in with Atlassian" button (shows "Waiting for the browser...",
Cancel), and "Use an API token instead" opening today's form. If the account reaches several
sites, a site picker (the chosen `cloudId` is kept). The Jira card says how it is connected.
"Reconnect" re-runs the sign-in. Expired and refused sign-ins show "Reconnect", as the token
form does. The Jira settings page shows the method and site.

### J7.5 Hardening and guide
Threat model update: loopback listener (bound to 127.0.0.1, single use, `state` and PKCE),
token storage, tool drift, an organisation blocking the redirect (a clear message, and the
fallback), credit use. Update `status.md` and `hardening.md`. Manual guide: sign in, wrong
account, closing the browser tab, several sites, revoke from Atlassian's side, token fallback,
org-blocked redirect.

## Decisions needed

1. **Run J7.0 first?** Recommended. It costs an hour and tells us if this works for your org.
2. **Keep the API-token fallback?** Recommended yes. It is already built and works where the
   redirect is blocked.
3. **Rovo credits:** Atlassian's MCP calls count against the organisation's credits. Fine for you?

## Sources

- Atlassian Rovo MCP server, repository: https://github.com/atlassian/atlassian-mcp-server
- Getting started: https://support.atlassian.com/atlassian-rovo-mcp-server/docs/getting-started-with-the-atlassian-remote-mcp-server/
- Supported tools: https://support.atlassian.com/atlassian-rovo-mcp-server/docs/supported-tools/
- OAuth 2.1 guide: https://developer.atlassian.com/cloud/rovo-mcp/guides/configuring-oauth-2-1/
- Authorization server metadata: https://auth.atlassian.com/VCeDsk8ZHncYF1g234fKtc4lNipbBhu3/.well-known/oauth-authorization-server
- Protected resource metadata: https://mcp.atlassian.com/.well-known/oauth-protected-resource/v2/mcp
- Custom OAuth apps and redirect URIs request: https://jira.atlassian.com/browse/ROVO-870
- Atlassian 3LO (client secret required): https://developer.atlassian.com/cloud/jira/platform/oauth-2-3lo-apps/
