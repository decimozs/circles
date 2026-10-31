---
title: "Chats"
description: "Talk to a Claude Code agent: the message box, permissions, checkpoints and side questions."
---

A chat is a conversation with one Claude Code agent inside a workspace. A workspace can have several chats at once, each in its own tab. Closed chats stay in the chat history of the top bar.

## The message box

The box floats over the bottom of the conversation. Messages scroll under it and show through, blurred, and the last message can always be scrolled clear of it. When you have scrolled up, **Scroll to end** appears above the box and takes you back to the newest message. If your Mac is set to reduce transparency, the box is solid.

- **Send** with Enter or Cmd+Enter. Settings > General chooses which one sends and which one breaks the line.
- **`@`** mentions a file. Circles searches the workspace as you type.
- **`/`** starts a slash command: your project's and your own `.claude/commands`, your skills, and built-ins such as `/add-dir`.
- **Attachments:** the `+` button, drag and drop, or paste. Pasted text over 5,000 characters becomes an attachment so it does not fill the box (Settings > General). An attachment can be at most 25 MB.
- **Model and effort** pickers, and toggles for plan mode and fast mode. The icon next to the model is the Claude mark for Claude models.
- **Next message suggestion:** after an answer, the empty box can show a likely next message. Press Tab to use it. It is never sent for you (Settings > Smart titles).

## While the agent works

The send button becomes **Stop**. A message you send while it works either **steers** the current turn or waits in a **queue**, as set in Settings > General (Follow-up behaviour).

## Permissions

A chat has a permission mode that decides what asks first:

| Mode | What it does |
|---|---|
| Ask | Edits and commands each ask first. |
| Accept edits | Edits go through, other things still ask. |
| Plan | The agent plans and does not change anything. |
| Full access | Nothing asks. A warning is shown when you pick it. |

New chats start in the mode set in Settings > Agents > Permission mode. A request waits in the chat as a card until you allow or deny it. Allowed means the agent can do it as you.

## Checkpoints

Before every message you send, Circles saves a snapshot of the workspace (tracked, modified and untracked files, not ignored ones). It is stored in private git refs, so your branch history and index are not touched. Hover a message and press **Revert** to go back: you see the files that will change, and the turn and everything after it are removed from the chat. Circles saves the state before a revert, so a revert can itself be undone for a while. **Branch conversation from here** starts a new chat from that point. Revert is blocked while a turn is running, and you are warned if another chat in the workspace is active. Checkpoints are removed after the time set in Settings > Advanced.

## Side questions

Type `/btw` followed by a question, or use its button, to ask something about the chat without interrupting the agent. The answer appears in a small pane above the box and is not added to the chat. Settings > Agents chooses the model and whether it may read files. It can never change anything.

## Titles

A new chat gets a short title from what you asked. Claude Haiku writes it, through Claude Code: it spends a little of your Claude usage and sends the start of your first message to Anthropic. The first words of the message are the title until Haiku answers, and a name you gave a chat is never replaced. You can switch titles off in Settings > Smart titles. Without Claude Code the first words stay.

## Cards

If you allow it (Settings > Agents), a chat can show a small card in the conversation to ask you to choose or fill something in, such as a checklist or a form, and read your answer back. A card waits ten minutes for an answer.

## Notifications

When Circles is in the background it can send a system notification when an agent finishes or needs you, with a sound if you want one (Settings > General). Clicking it cannot open the chat yet.
