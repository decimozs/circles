---
title: "Troubleshooting"
description: "What to do when macOS will not open Circles, or something does not work."
---

**macOS will not open Circles ("cannot be opened", "unidentified developer" or "is damaged")**
The app is not signed by an Apple Developer account yet. The first time: right-click the app in
Applications, choose Open, then Open again. Or open System Settings, Privacy & Security, scroll to
the message about Circles and choose Open Anyway. If macOS says the app is damaged, the download
was marked as quarantined: run `xattr -dr com.apple.quarantine /Applications/Circles.app`.

**A notification says it is from Script Editor**
Circles could not send it itself and used a script. Allow notifications in System Settings,
Notifications, and use the installed app rather than a development build.

**macOS asks again and again for a Keychain password**
A development build is a new program every time it is built, so macOS asks again. The installed
app asks once: choose Always Allow.

**Chats are not named, or the next message is not suggested**
Titles and suggestions use Claude Haiku through Claude Code, so Claude Code has to be installed and signed in (see "Claude Code was not found" below). Settings > Smart titles has a **Try it** button that shows whether it answers, and the switches for each job.

**"Claude Code was not found"**
Install it from https://claude.com/claude-code, run `claude` once in a terminal to sign in, then
use Settings > Agents > Check. If it lives somewhere unusual, set its path there.

**"The GitHub CLI is not signed in"**
Run `gh auth login` in a terminal. Install the CLI first with `brew install gh` if needed.

**git is missing**
Run `xcode-select --install`.

**"Claude Code cannot find this conversation" after changing the Claude account**
Claude Code keeps each conversation in its config folder (`CLAUDE_CONFIG_DIR`, `~/.claude` by default). A chat that began under one account or folder cannot be resumed under another. Send your message again: Circles continues in a new session and tells it what was said before. Tool output from earlier turns is not carried over, only what was said and which tools were used.

**A workspace shows an error after a crash or restart**
Open it and press Restore. Circles repairs a missing worktree from the branch.

**A script or agent cannot find a tool that works in my terminal**
Circles reads the environment from your login shell at startup. Settings > Environment shows
what is set, and the diagnostics report lists which tools were found.

**Reset**
Settings > Advanced > Reset app data deletes chats and workspace records but keeps your settings
and repositories.

**Reporting a bug**
Settings > Advanced > Copy diagnostics, or Export logs. Both remove secrets first. Logs live in
`~/Library/Application Support/circles/logs`.
