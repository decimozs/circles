---
title: "Composers"
description: "A manager agent that starts workspaces and agents for you, and how to follow its run."
---

A Composer is a manager agent. You give it a goal in plain words, and it uses Circles' own actions to look at your projects, create workspaces, start coding agents in them, follow their progress and open pull requests. It does not edit code itself: the agents it starts do.

## Making one

Open **Composers** in the sidebar and press **New Composer**, or start from a template (for example **Task splitter**).

- **Name** and **Mode.** The mode works like a chat's permission mode (Ask, Accept edits, Plan, Full access) and is the starting point for what the Composer may do on its own. New Composers start in the mode set in Settings > Agents.
- **Instructions:** what it should do, how, and what it must never do. Markdown works, and typing `/` offers your skills. The **Improve** button rewrites rough instructions as clear Markdown. See below.
- **Repositories:** all of your projects, or only the ones you pick. It cannot see or change any other.
- **Integrations:** a switch for Jira. Off, the Composer has no Jira tools at all. See [Jira](./jira.md).

### Improve

The **Improve** button beside the Markdown, Write and Preview controls has Claude rewrite what you typed: the goal first, then the steps in order, then what must never happen. It keeps every fact, name, ticket key, repository and limit, adds none, keeps `/skill` and `@` mentions as written, and answers in the language of the text. While it works the box is disabled and pulses slowly, and the button says "Improving". The result replaces the text and **Undo** brings the old text back. A failure leaves the text as it was, and nothing is saved until you save the Composer.

It runs once with no tools, away from any project and without your own Claude settings. It uses your Claude subscription and sends the text to Anthropic, only when you press the button.

## Running one

Open a Composer and type a goal, for example "List my projects and tell me what each one is." Each start is a **run**, shown as a chat tab. The run page has no right pane. A button at the top right (a list icon) shows or hides a panel that stays open while you work in the chat, and the choice is remembered. The button shows a small spinner while anything of the run works, and the number of approvals waiting for you. The panel has:

- **Fleet:** the workspaces the run started, live. Each row shows what its agent is doing: a spinner while it works, an amber dot while it waits for your answer, a tick when it is done. The time beside a working row counts up from when its current turn began. What waits for you comes first, then what works. Finished ones are folded under **Done**. Click a row to open that workspace, or the arrow at its right to open out what it holds: **Open in** an editor (with a menu for the others), its branch and how many commits it is ahead or behind, its pull request with its checks and the next step (Create PR, Fix checks, Merge and so on, as in the workspace's top bar), and **Changes** with how many lines, which opens its Changes tab. These are asked from GitHub only while a row is open. **Fleet graph** opens the run as a graph in a dialog.
- **Approvals:** what is waiting for your answer, with the same Allow, Allow for this run and Deny buttons as the card in the chat, so you answer without leaving the panel. What was answered lately is folded under **Answered**. The section is not there for a run that never asked.
- **Repositories** and **Integrations:** which of your repositories the Composer can see, and whether Jira is on for it and connected.
- **Settings:** the Composer's instructions, notes, autonomy and limits.

A Composer keeps notes between runs, and closed runs stay in the chat history.

## What it may do on its own

Every tool has a tier and an autonomy setting:

| Tier | Examples | Default |
|---|---|---|
| Read | list projects, workspaces and pull requests, read a chat or its changes, wait for chats, check a pull request | Allowed |
| Write | create a workspace, start a chat, send a message, interrupt a chat, add or clone a project, rename a branch | Asks |
| External | create a pull request (it pushes the branch), merge a pull request | Asks |
| Destructive | archive or delete a workspace, remove a project | Asks |

Each tool is **ask** (an approval card each time), **allow** or **off** (the Composer does not have it). The mode and templates set the starting point, and the Details tab shows the list, with a warning on any external or destructive tool set to allow. Merging follows the same rules as the Merge button: failing checks, conflicts and requested changes block it, and overriding those is a separate switch that is off by default.

An approval appears above the input and in the Approvals tab, and the Composers item in the sidebar shows a count. A tool that was denied is not retried. Every call is in the **audit log**, which you can search and export as CSV.

## Limits and stopping

A run has no limit on tool calls, time, or how many workspaces and chats it starts. A person can stop it, and Claude Code's rate limits apply. The one limit you can set is an optional **spending limit** per request, passed to Claude Code as its own limit.

- **Stop** a run, or one agent it started, from its page.
- **Stop all Composers** (Cmd+Shift+.) stops every run and its agents after you confirm.
- **Pause Composers** on the Composers home refuses every tool call until you resume.

## Running on their own

A Composer can start by itself:

- **Schedules:** daily at a time, or every N minutes.
- **Triggers:** a new pull request, a failing check, or a Jira query.

Anything set to ask waits as an approval. Runs started this way show a clock icon in the sidebar. They are capped per day (Settings > Composers, 20 by default) and a trigger that fails five times in a row is switched off. A run missed while Circles was closed still starts if it is under 12 hours late, once. GitHub is asked about each trigger every 2 minutes, and the first look only records what exists, so old pull requests start nothing.

Settings > Composers lets Circles keep running in the menu bar, open at login and keep the Mac awake during such runs, and notify you when something waits.

## Sharing a Composer

A repository can keep Composers in `.circles/composers/*.toml` with a `name`, `instructions` and optionally `tools` and `[limits]`. They appear under "In your repositories" on the Composers home, and you can add them to your own. Autonomy is never read from a file, so a shared file cannot make a Composer more powerful than your settings allow.

## How safe is it

The app, not the model, decides what a Composer may do: every tool call is checked against its settings, and a Composer cannot change its own permissions. Text that comes from outside, such as a pull request or a Jira ticket, is labelled as untrusted data in what the model sees. Only Claude is supported as the model.
