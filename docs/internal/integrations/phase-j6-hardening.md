# J6: Hardening, triggers and the manual guide

- **Trigger** `jira_issue` (wanted): a JQL query polled like the GitHub triggers (baseline first, dedupe, daily cap,
  five failures switch it off). Text wrapped as untrusted. Offered in the trigger editor only when Jira is connected and on for that Composer.
- Token checks again: grep logs and test output for it, crash path, export, diagnostics bundle.
- 401 mid-run: the tool says "Reconnect Jira in Settings", the card shows Needs reconnecting.
- Update `docs/hardening.md`, `docs/composers/status.md`, `CHANGELOG.md`, and add `docs/integrations/status.md`.
- **Manual guide** for a real Jira Cloud site: connect, wrong token, search, read, comment (asks), transition, a Composer with Jira
  off, Composer limited to one repository, a chat reading a ticket, disconnect, reconnect.

**Done when:** all checks green and the user has run the guide.
