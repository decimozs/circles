# Changelog

## Unreleased

### Changed
- The repository is a monorepo: the app is in `apps/desktop`, shared code in `packages/ui`, `packages/bindings` and `packages/config`.
- Release builds target macOS 11 or later, which the bundled model runtime needs.

### Added
- Branch naming rule per project (Settings, Git, `branch_pattern`, for example `{ticket}-{slug}`): Composers name branches by it, and `list_projects` shows it to them.
- Project skills: agents in every workspace are told about `.claude/skills`, `.agents/skills`, `.agent/skills`, `AGENTS.md` and `CLAUDE.md`, and a Composer no longer reads your user level instructions.
- Cards from agents: a Composer (and, if switched on, any chat) can show a small checklist, form or plan in the chat and read your answer.
- Side questions: `/btw` (or ⌘⇧B) asks a quick question about a chat and answers in a pane above the message box, without changing the chat or interrupting the agent.
- Smart chat titles: a small model that ships inside Circles (SmolLM2-360M, nothing to install, nothing sent anywhere) names new chats, and can suggest the next message as the input placeholder.
- Sign in with Atlassian: connect Jira in the browser instead of pasting a token. The API token stays as a fallback.
- Jira Cloud integration: connect in Settings, Integrations (token in the Keychain). Every chat can search and read issues, and change them after a permission prompt. Composers get a Jira switch, a choice of repositories, and a Jira trigger.
- Local desktop app that runs Claude Code agents in parallel, each in its own git worktree.
- Projects, workspaces, sections, a command palette and a Home screen.
- Terminal dock with setup, run and archive scripts.
- Chat with Claude Code: streaming, tools, permissions, plan mode, attachments, queued and steered messages.
- Files, Changes, diff view with comments, checks, todos and pull requests through `gh`.
- Checkpoints: revert a chat and the files to before any turn, undo, and branch from a turn.
- Settings pages backed by `settings.toml` at user, project and local level, with live reload.
- First-run flow with a tool check and a security notice.
- Content Security Policy, "copy diagnostics" and an error screen with a way to recover.
- Last-used time on every sidebar workspace row.
- Cmd+Shift+M maximises and restores the terminal.
- Setting for how long checkpoints are kept.

- Composers: manager agents with their own tools to create projects and workspaces, start agents, follow them and open or merge pull requests, with per-tool autonomy, approvals, limits, an audit log, a kill switch, schedules and triggers.

### Changed
- The Changes tab opens on uncommitted changes.
- A default model in the settings is applied when it changes, so a later pick in the Create dialog is kept.
- Model selector shows the Anthropic mark, and the file tree shows an icon per language.
