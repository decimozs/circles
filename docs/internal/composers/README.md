# Composers: Phase Plan

A **Composer** is a manager agent inside Circles. You give it a goal, and it uses Circles' own actions to create projects and workspaces, start coding agents in them, watch their progress, and report back. The coding agents are the same Claude Code agents Circles already runs. The Composer decides what to start and when.

This is a plan, not built yet. It follows the same rules as `docs/phases/`: each phase is a separate unit of work, stop for review after each one, and nothing is committed or pushed unless the owner asks.

## Proposed decisions (to confirm in C0)

| Topic | Proposal |
|---|---|
| Name | Composer (a definition) and Run (one execution of it) |
| What it is | A tool-calling loop over Circles' own services. It does not edit files itself. |
| Engine | The loop is written in Rust, inside the app. No LangGraph or other agent framework. |
| Tools | One typed **tool registry** is the source of truth. Everything else is an adapter over it. |
| Brain for now | **Claude only** (decided). Claude Code, given the registry through a small local MCP adapter. No API keys needed. |
| Other brains | Deferred. OpenAI, Grok (xAI) and Anthropic through their APIs, using the registry directly with no MCP, are phase C4, which waits until Claude-only Composers have proven themselves. No provider library is chosen. |
| Resumable | Every step of a run is stored. A run can resume after the app restarts. |
| Keys | API keys live in the macOS Keychain, never in `settings.toml`. |
| Safety | Tools have four tiers. `external` and `destructive` tools ask by default, and the person can allow each one per Composer (and per project). Nothing is silent unless the person turned it off for that Composer. |
| Modes | Both **attended** (you start and watch) and **unattended** (triggered or scheduled, runs while you are away). Both are required. |
| Scope | Local only. No cloud, no teams. Unattended runs need the app to be running. |

## Words used

- **Tool:** one action a Composer can take, such as `create_workspace`. It has a name, typed inputs, and a tier.
- **Tier:** `read` (look only), `write` (local and reversible), `external` (visible outside this Mac: push, open a pull request, comment) and `destructive` (delete, merge, remove).
- **Run:** one execution of a Composer, made of recorded **steps**: a model reply, a tool call, a tool result, an approval.
- **Child:** a workspace or chat that a run started. It remembers which run did.
- **Fleet:** the children of a run, shown together.

## Phases

| # | Phase | File |
|---|---|---|
| C0 | Spec and threat model | [phase-c0-spec.md](phase-c0-spec.md) |
| C1 | Tool registry and audit log | [phase-c1-tool-registry.md](phase-c1-tool-registry.md) |
| C2 | Composer v1 on Claude | [phase-c2-composer-v1.md](phase-c2-composer-v1.md) |
| C3 | Fleet orchestration | [phase-c3-fleet.md](phase-c3-fleet.md) |
| C4 | Providers and keys (deferred) | [phase-c4-providers.md](phase-c4-providers.md) |
| C5 | Library and templates | [phase-c5-library.md](phase-c5-library.md) |
| C6 | Triggers and unattended runs (required, after C3) | [phase-c6-triggers.md](phase-c6-triggers.md) |
| C7 | Hardening | [phase-c7-hardening.md](phase-c7-hardening.md) |

Order matters: C1 before everything, C2 before C3. C4 is deferred. Because unattended runs are a must, **C6 comes right after C3**, before C5 and C7. Recommended order: C0, C1, C2, C3, C6, C5, C7.

## Autonomy

How much a Composer may do without asking is the person's choice, per Composer. It is set by a **mode**, the same four a chat has, chosen when the Composer is made. A new Composer starts in the mode named by Settings > Agents > Permission mode, like a chat does, and manual when none is set.

| Mode | What it does with a tool |
|---|---|
| Ask (`manual`) | Looking is free. Everything else asks. |
| Accept edits (`accept_edits`) | Looking and changes on this Mac go ahead. Opening or merging pull requests, and deleting, ask. |
| Plan (`plan`) | Looking only. The other tools are not given to it. |
| Full access (`bypass`) | Nothing asks. Merging past failing checks is still its own switch, off by default. |

A tool with a setting of its own (a template can switch tools off, for example) keeps it whatever the mode says.

- **Per tool:** each tool of a Composer is `ask` (a card each time), `allow` (does it without asking) or `off` (the Composer does not have it). Defaults: `read` allow, `write` ask, `external` ask, `destructive` ask.
- **Per project:** a Composer can be limited to certain projects, and a tool can be allowed in some and ask in others.
- **Merging:** `merge_pr` still follows the same readiness rules as the merge button: failing checks, conflicts and requested changes block it. Overriding those blockers is a separate switch, off by default.
- **Visible:** enabling `allow` on an `external` or `destructive` tool shows a plain warning, and the Composer's page shows its autonomy at a glance. Every call is in the audit log either way.
- **Always on:** the optional spending limit and the stop-all key apply no matter what is allowed. A run has no limit on tool calls, time, or how many workspaces and chats it starts: a person can stop it, and Claude Code's rate limits apply.

## What is built

All phases except C4 (deferred) are built. See [status.md](status.md) for what each phase delivered, how to try it, and what is still open.

## UI

See [ui.md](ui.md): a Composers item below Create, a Composers group in the sidebar laid out like projects and workspaces (with pinning), the same center and right panes with Composer tabs (Fleet, Approvals, Steps, Details), and a Composers page in Settings under Agents & environment.

## Open questions

1. ~~Is Claude-first acceptable?~~ **Answered: yes. Composers use Claude only for now.** C4 is deferred.
2. ~~May a Composer create new projects?~~ **Answered: yes. It can create new projects and also create workspaces in existing ones.** Project creation is a `write`-tier tool that asks by default, like the others.
3. ~~Are unattended runs a goal?~~ **Answered: yes, a must.** Composers work both attended (you start and watch) and unattended (they start on their own and run while you are away). C6 is required.
4. ~~May Composers merge pull requests?~~ **Answered: yes, it depends on what the person wants.** Every Composer has an autonomy setting per tool. See "Autonomy" below.
5. ~~Keeping the app alive for unattended runs~~ **Answered: the menu bar option, as settings that are off by default (stay alive in the menu bar, start at login, keep the Mac awake during a run). A separate always-on background service is out of scope for now.**

## How it fits what exists

| Existing piece | Used for |
|---|---|
| `workspace::service`, `project::service` | The services the registry wraps. No new way to create a workspace. |
| `harness::sessions` and `ClaudeCode` | Running the child agents, and the first brain. |
| Permission cards (`permission_reply`) | The shape of approvals for `external` and `destructive` tools. |
| `settings` layers and `settings.toml` | Composer limits and defaults. |
| Checkpoints | A safety net for child workspaces a Composer changes. |
| `logging::Redactor` | Keeping secrets out of the run log and tool results. |
| Concurrency soft cap (deferred in phase 13) | Becomes real in C3. |
