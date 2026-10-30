# J2: Settings, Integrations

**Output:** a new settings page.

- `lib/settings-pages.ts`: page "Integrations". `features/settings/integrations-page.tsx`.
- Jira card: official logo (`SiJira` from `react-icons/si`, coloured with the `--brand-jira` token), name,
  one line of what it does, status (Connected as Name at site / Needs reconnecting / Not connected).
- Buttons: **Connect** (dialog: site, email, API token, link to Atlassian's token page, the permissions warning),
  **Reconnect** (same dialog, site and email filled, token empty), **Disconnect** (confirms).
- A switch "Let chats use Jira" (default on once connected), used by J5.
- Errors shown inline and plainly: wrong site, wrong token, no network.
- The token field is a password input, cleared after saving, never kept in a store or draft.

**Tests:** component tests with FakeBackend for each state and button; axe in both themes; the token is
not in the DOM after save.
