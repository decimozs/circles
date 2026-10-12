# J4: Composer dialog: repositories and integrations

**Output:** two sections in the New and Edit Composer dialog.

- **Repositories:** a choice of "All repositories" (`projects = null`) or "Only these", with a searchable
  checklist of existing projects (`projects = [ids]`). Enforcement already exists. New: when "Only these" has
  none picked, Create is disabled with a plain reason.
  Edited Composers keep what they have.
- **Integrations:** a row with the Jira logo and a switch. Disabled with a "Connect Jira in Settings" link when not connected.
  Stored as `composers.integrations` (migration 0011, JSON array, default `[]`). Shared `.circles/composers/*.toml` files
  may switch it off but never on, like tools.
- Keep the dialog from overflowing, with the name and mode row at 80/20 as it is now.
- Templates can seed `integrations`.

**Tests:** form payloads, disabled states, a migration test, shared file cannot enable Jira.
