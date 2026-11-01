---
title: "Circles guide"
description: "Circles runs Claude Code agents in parallel, each in its own copy of your repository, on your Mac."
---

Circles is a Mac app that runs Claude Code agents in parallel. Each agent works in its own copy of your repository, on its own branch, so several can work at once without getting in each other's way. Everything runs on your Mac.

## Read in this order

1. [Getting started](./getting-started.md): install, the setup checklist, your first project and first chat.
2. [Projects](./projects.md): adding repositories, the GitHub picker, and how Circles learns a repository's habits.
3. [Workspaces](./workspaces.md): what a workspace is, its files, changes and graph, the terminal and scripts.
4. [Chats](./chats.md): talking to an agent, permissions, checkpoints and side questions.
5. [Pull requests](./pull-requests.md): drafting, checks, and merging.
6. [Skills](./skills.md): a directory of the skills on this Mac and in your projects.
7. [Composers](./composers.md): a manager agent that starts and follows other agents for you.
8. [Jira](./jira.md): letting chats and Composers read and work with Jira.
9. [Settings](./settings.md): where settings live, and what each page holds.
10. [Usage](./usage.md): tokens and estimated cost of Claude Code, and your plan limits.
11. [Keyboard shortcuts](./shortcuts.md).
12. [Troubleshooting](./troubleshooting.md).

## Words used in this guide

| Word | Meaning |
|---|---|
| Project | A git repository you added to Circles. |
| Workspace | A copy of a project (a git worktree) on its own branch, where agents work. |
| Chat | A conversation with one Claude Code agent inside a workspace. A workspace can have several. |
| Composer | A manager agent. You give it a goal and it creates workspaces and starts agents. |
| Checkpoint | A snapshot of a workspace taken before each of your messages, so you can go back. |
