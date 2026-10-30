# J0: Spec and threat model

**Output:** `docs/integrations/spec.md`, `threat-model.md`. No code.

- List the tools with tier, inputs and outputs (see J3).
- Threat model: ticket text injecting instructions; a token leaking through logs, env, MCP config,
  crash reports, exports of shared Composer files; a chat or Composer reaching a site that is not the
  connected one (the base URL is fixed at connect time, never taken from a tool input); SSRF through
  attachment or link fields (no URL-taking tools in v1).
- Decide the permission mapping for chats (J5) and write it down.
- Confirm the logo source: the Simple Icons `SiJira` mark with Atlassian blue, as a `--brand-jira`
  token in `tokens.css` (raw colours are only allowed there).

**Done when:** the user has read and approved the spec.

Storage note: Keychain, not a hash (a hash cannot be used to call Jira). Document in the threat model what the Keychain does and does not protect against.
