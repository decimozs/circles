---
title: Getting started
description: Install Circles, finish the setup checklist, add a project and start your first chat.
---

## Install

Build the app with `scripts/release.sh` and open the `.dmg`, or run it from source with `pnpm tauri dev`. From source you need macOS, Node 24, pnpm, Rust stable and the Xcode Command Line Tools.

The app is not signed yet, because there is no Apple Developer account. The first time, macOS will not open it with a double click: right-click the app, choose **Open**, and confirm. If macOS says it is damaged, run `xattr -dr com.apple.quarantine /Applications/Circles.app` once. More in [Troubleshooting](troubleshooting.md).

## What you need on your Mac

| Tool | Why |
|---|---|
| `git` | Every workspace is a git worktree. Install it with `xcode-select --install`. |
| Claude Code (`claude`) | The agent. Install it from https://claude.com/claude-code and run `claude` once to sign in. |
| GitHub CLI (`gh`) | Listing and cloning repositories, and pull requests. Install with `brew install gh`, then `gh auth login`. |

## The setup checklist

On first launch Circles shows a checklist: `git`, Claude Code (installed and signed in), the GitHub CLI, notifications, the folders where your code lives, and a notice that agents run with your permissions. The required items must be ready before you add your first project.

Agents are Claude Code running as you. They can read and change what you can. Each chat has a permission mode that decides what asks first (see [Chats](chats.md)).

## Your first project and first chat

1. Press **Add project** (Cmd+Option+A) and choose a local folder, a GitHub repository, or a quick start. See [Projects](projects.md).
2. Press **Create workspace** (Cmd+N). Pick the project, type what you want to work on, and press Enter. Circles makes a workspace on a new branch and starts a chat in it.
3. Watch the agent work in the chat. Look at its files and changes in the right pane. When the changes look right, open a pull request ([Pull requests](pull-requests.md)).

## Where Circles keeps things

| What | Where |
|---|---|
| App data and your settings | `~/Library/Application Support/circles/` |
| Clones made from GitHub | `~/circles/repos/` |
| Workspaces | `~/circles/workspaces/<project>/<name>` |
| Logs | `~/Library/Application Support/circles/logs` |

Nothing leaves your Mac except what Claude Code and `gh` send on their own, and the features that call a model say so where you use them.
