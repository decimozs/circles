# Jira

Circles can let every chat, and every Composer that you allow, read and work with Jira Cloud. Jira Server and Data Center are not supported.

## Connecting

Open Settings > Integrations and press **Connect Jira**.

- **Sign in with Atlassian** opens your browser. Circles asks for Jira read, write and search access, plus offline access so the sign-in can be renewed. It never asks to delete or manage anything. If your account has several Jira sites, you pick one.
- **Use an API token instead** takes a site (`name.atlassian.net`), your email and an API token. Circles checks them before saving. An API token has every permission of the account it belongs to, so a token of a dedicated or limited account is safer.

Either way, the secret is kept in the macOS Keychain. It is never in a settings file, the database, a log or an export, and Claude never sees it: agents ask Circles, and Circles calls Jira. The Jira card shows the site, the account and whether the connection works. If a sign-in can no longer be renewed, it asks you to sign in again. **Disconnect** removes the saved sign-in from your Mac but does not revoke it at Atlassian. Revoke it at id.atlassian.com under connected apps if you want it gone there too.

Your organization can block the sign-in redirect or the app. Then use the API token form.

## In chats

With Jira connected, **Settings > Jira > Let chats use Jira** is on by default, and every new chat gets the Jira tools. Ask "what is PROJ-123 about?" or "comment on it" in a chat. It takes effect for chats that start after you change it.

## In Composers

The Composer dialog has a **Jira** switch under Integrations. Off, that Composer has no Jira tools at all. It is disabled until Jira is connected, and disconnecting switches it off on every Composer. A shared Composer file can never switch Jira on. A Composer can also have a **Jira trigger** that runs a JQL query and starts a run for what it finds.

## What the tools can do

| Tools | Asks first? |
|---|---|
| Search issues, read an issue, list projects, see the transitions and the fields needed to create an issue | No |
| Add a comment, move an issue to another status, assign it, edit it, create an issue | Yes, like any other change |

There is no tool to delete anything. Text that comes from Jira, such as a ticket's description or a comment, is labelled as untrusted data when the model sees it, because ticket text is the likeliest place for a prompt injection. Keys and ids are checked before they reach an address, and results are capped in size.

Calls through Atlassian's sign-in use your organization's Rovo credits, and Atlassian documents that server as beta. Jira rate limits (429) show up as a tool error and are not retried in a loop.
