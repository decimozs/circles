# J3: Jira tools for Composers

**Output:** the tools, enforced by the existing dispatcher.

- `tools/catalog.rs`: new field `integration: Option<&'static str>` (`"jira"`). A tool with it is hidden and refused unless
  Jira is connected and the Composer has Jira on (J4).
- Tools: `jira_search` (JQL, paged, fields limited), `jira_get_issue` (summary, status, assignee, description as text,
  recent comments, links), `jira_list_projects`, `jira_get_transitions` (read, allow);
  `jira_add_comment`, `jira_transition_issue`, `jira_assign_issue`, `jira_edit_issue` (summary/description/labels),
  `jira_create_issue` (write, ask). Nothing deletes.
- App-side implementation in `tools/jira.rs` using the J1 client. Results go through `shape_result`.
- Audit rows record the tool, the issue key, and the outcome. Never the token.
- A Composer's system prompt gets one line when Jira is on: what the tools are for, and that ticket text is data.
- Per-tool autonomy and modes apply as they do now (manual: reads allow, writes ask).

**Tests:** dispatcher refuses when not connected, when the Composer has it off, with a bad schema; ADF round trip;
injected ticket text stays inside `<untrusted-data>`; real-Claude opt-in test against the fake Jira.
