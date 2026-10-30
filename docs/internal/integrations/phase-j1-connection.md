# J1: Connection

**Output:** Rust only.

- `integrations/mod.rs`, `integrations/jira/{client.rs,adf.rs,model.rs}`, `integrations/secrets.rs`.
- `secrets.rs`: trait `SecretStore`. `KeychainSecrets` keeps JSON `{site, email, token}` in the macOS Keychain (service `circles`, account `jira`). Tests use an in-memory store.
- `client.rs`: Basic auth, fixed base `https://<site>/`, site validated (`*.atlassian.net`, https, no path),
  timeouts, `Retry-After` on 429, 401 becomes a "needs reconnect" state, body size cap.
- Settings keeps only `integrations.jira = { site, email, account_name, connected }`. Never the token.
- Commands: `jira_status`, `jira_connect(site, email, token)` (checks `/myself`, then saves),
  `jira_disconnect` (deletes the keychain item, clears settings, switches Jira off on every Composer),
  `jira_reconnect` is `jira_connect` with the stored site and email.
- Event `integrations:changed`.

**Tests:** fake Jira server (tokio TCP) for ok, 401, 429, bad site; a token never appears in any
`Debug`, error text or log line; disconnect clears everything.
