# Integrations: Jira Cloud

Goal: every chat in Circles, and every Composer that is allowed to, can read and work with Jira.
Jira Cloud only. Gmail is out of scope. Server and Data Center are out of scope.

## What the person gets

1. **Settings, Integrations.** A Jira card with the official Jira logo. Connect, Reconnect and
   Disconnect. It shows the site, the account and whether the connection works.
2. **Every chat can use Jira.** Ask a workspace chat "what is PROJ-123 about" or "comment on it" and it can.
   Reading is free. Changing Jira asks first, like any other change.
3. **New Composer dialog.**
   - **Repositories:** all existing repositories, or only the ones picked. (The check already exists
     in the dispatcher. The choice was hidden. It comes back.)
   - **Integrations:** a switch for Jira. Off, the Composer has no Jira tools at all.

## Decisions

- **Auth:** site, email and API token. OAuth needs an Atlassian app that Circles would have to
  register and ship. A local app should not. The token is checked against `GET /rest/api/3/myself`.
- **Where the token lives:** the macOS Keychain (`keyring` crate), the stronger place and the
  first choice. Hashing was considered and cannot work: every call to Jira needs the real token,
  and a hash cannot be turned back. If the Keychain is ever unavailable, the fallback is an
  encrypted value in the database with a key file, never a hash. That fallback is not built now.
  The code uses a `SecretStore` trait, so it can be added without changing anything else.
  The token is never in `settings.toml`, the database, an env var, the MCP config file, a log, the
  audit log, checkpoints, exports or diagnostics. Only the app process reads it. Claude, the MCP
  helper and the Composer never see it. They ask the app, and the app calls Jira.
- **One Jira layer, two doors.** The Jira tools are written once, in the app. Composers reach them
  through the existing Composer MCP socket. Chats reach them through a second MCP entry for chats.
- **Tiers:** read allowed. Comment, transition, create, assign and edit ask. No delete tool.
- **Data from Jira is untrusted.** Wrapped in `<untrusted-data>`, fake closing tags defused,
  20k cut, secrets redacted, like pull request text. Ticket text is the likeliest prompt injection.
- **Warning to show:** an API token has all the permissions of the account. Suggest a token of a
  dedicated or limited account.

## Phases

| Phase | What | Output |
|-------|------|--------|
| [J0](phase-j0-spec.md) | Spec, threat model, scope | docs only |
| [J1](phase-j1-connection.md) | Keychain, Jira client, connect commands | Rust, tested against a fake Jira |
| [J2](phase-j2-settings.md) | Settings, Integrations page | UI with logo |
| [J3](phase-j3-tools.md) | Jira tools in the catalog, dispatcher, audit | Composers can use Jira |
| [J4](phase-j4-composer-dialog.md) | Repositories choice, Integrations switch | dialog |
| [J5](phase-j5-chats.md) | Jira for every chat | the expected result |
| [J6](phase-j6-hardening.md) | Jira trigger, hardening, manual guide | done |

| [J7](phase-j7-atlassian-signin.md) | Sign in with Atlassian (OAuth through Atlassian's MCP server) | built, tested against a fake; real sign-in not yet seen working |

Order is fixed: J1, then J2 and J3 (independent), then J4, then J5, then J6.
Each phase ends green: Biome, typecheck, clippy, targeted tests. Nothing is called done on a real
Jira until the manual guide in J6 is run.

## Open risks

- **Atlassian Document Format.** Jira's descriptions and comments are ADF, not text. Reading needs
  ADF to text. Writing needs text (light Markdown) to ADF. Keep both small and well tested.
- **Search API.** Use the current `POST /rest/api/3/search/jql` with `nextPageToken`, not the old `/search`.
- **Rate limits.** Honour 429 and `Retry-After`. Surface it as a tool error, never loop.
- **Chats and permissions.** Chats have Claude's own permission prompts. Composer autonomy and chat
  permission mode are different systems. J5 maps one to the other and says exactly how.
