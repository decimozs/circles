# Circles: Phase Plan

Circles is a local-first desktop app (Tauri 2) that runs Claude Code agents in parallel, each in an isolated git worktree workspace. It clones the concepts of Conductor, locally only.

Each phase is a separate unit of work. After a phase is done, stop and wait for review before starting the next one. Nothing is committed or pushed unless the owner asks.

## Locked decisions

| Topic | Decision |
|---|---|
| Name | Circles |
| Shell | Tauri 2, macOS first |
| Frontend | React, TypeScript, Vite, Tailwind, shadcn/ui |
| Theme | light, dark, system (shadcn ThemeProvider, `.dark` class) |
| Diff and file viewer | Monaco |
| Harness | Claude Code only, behind a trait so others can be added later |
| GitHub | `gh` CLI only, no OAuth. No Linear. |
| Scope | Local only. No cloud, teams, or multiplayer. |
| Auth | Better Auth, deferred to phase 14. Hosting decision (hosted service vs local sidecar) is still open. |
| Git | Repo initialised, no commits or pushes until requested |
| Branch naming | `<gh-username>/<random5>`, auto-rename after first prompt |
| Base branch | `origin/HEAD`, overridable per project and per workspace |

## Data locations

| What | Where |
|---|---|
| Worktrees | `~/circles/workspaces/<project>/<city>` |
| App DB | `~/Library/Application Support/circles/circles.db` (SQLite) |
| User config | `~/Library/Application Support/circles/settings.toml` |
| Project config | `<repo>/.circles/settings.toml` (shareable) |
| Local project override | `~/Library/Application Support/circles/projects/<id>/settings.toml` |
| Private workspace notes | `<worktree>/.context/` (never committed) |
| Checkpoints | private git refs `refs/circles/checkpoints/<workspace>/<turn>` |

Settings precedence: local project override, then project, then user.

## Verified in the Claude Code spike

- `claude -p --input-format stream-json --output-format stream-json --verbose --include-partial-messages` gives structured events and token deltas over one persistent process.
- Interrupt: `control_request` with `subtype: "interrupt"`.
- Steer: writing a user message mid-turn is injected into the running turn.
- Permissions: `--permission-mode manual --permission-prompt-tool stdio` emits `can_use_tool` control requests that the host answers.
- Resume and fork: `--resume <id> --fork-session`. Forking is whole-session only, so checkpoint revert uses our own message log plus replay.
- A GUI app has no shell PATH. Resolve it from `zsh -ilc env` at startup.

## Phases

| # | Phase | File |
|---|---|---|
| 0 | Repo and tooling | [phase-00-repo-and-tooling.md](phase-00-repo-and-tooling.md) |
| 1 | Design system and theme | [phase-01-design-system.md](phase-01-design-system.md) |
| 2 | App shell | [phase-02-app-shell.md](phase-02-app-shell.md) |
| 3 | Rust core | [phase-03-rust-core.md](phase-03-rust-core.md) |
| 4 | Projects and Git | [phase-04-projects-and-git.md](phase-04-projects-and-git.md) |
| 5 | Workspaces | [phase-05-workspaces.md](phase-05-workspaces.md) |
| 6 | Home, Create, Command palette | [phase-06-home-create-palette.md](phase-06-home-create-palette.md) |
| 7 | Terminal and scripts | [phase-07-terminal-and-scripts.md](phase-07-terminal-and-scripts.md) |
| 8 | Claude Code harness | [phase-08-claude-harness.md](phase-08-claude-harness.md) |
| 9 | Chat UI | [phase-09-chat-ui.md](phase-09-chat-ui.md) |
| 10 | Files, Changes, Monaco diff | [phase-10-files-changes-diff.md](phase-10-files-changes-diff.md) |
| 11 | Checks, Todos, PRs | [phase-11-checks-todos-prs.md](phase-11-checks-todos-prs.md) |
| 12 | Checkpoints | [phase-12-checkpoints.md](phase-12-checkpoints.md) |
| 13 | Settings pages | [phase-13-settings.md](phase-13-settings.md) |
| 14 | Auth (Better Auth) | [phase-14-auth.md](phase-14-auth.md) |
| 15 | Hardening and packaging | [phase-15-hardening-and-packaging.md](phase-15-hardening-and-packaging.md) |

## Phase file format

Every phase file has the same sections: Goal, Depends on, In scope, Out of scope, Deliverables, Technical notes, Done checklist, Manual test script.

## Dependency overview

```
0 -> 1 -> 2 -> 6
0 -> 3 -> 4 -> 5 -> 6
5 -> 7 -> 8 -> 9 -> 12
5 -> 10 -> 11
2 + 3 -> 13
all -> 14 -> 15
```

Phases 1-2 (UI-only) and 3-5 (backend) can be reviewed independently. Phase 6 is the first one where UI and backend meet.
