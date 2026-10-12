# Phase 17: Side questions (`/btw`)

Status: B0 to B3 built and tested. See "Built" and "Gaps" at the end.

## Goal

While a chat is open, even while its agent is working, you can ask a quick side question and read
the answer in a small pane above the message box. The question and answer are not added to the
chat, the agent is not interrupted, and the pane can be closed and forgotten.

Typing `/btw what does this function do?` and pressing Enter does it. A button and a shortcut do
the same.

## Why not Claude Code's own `/btw`

Claude Code's `/btw` draws an overlay on its terminal screen. Circles talks to it through its
streaming interface, which has no screen, so there is no overlay to appear. Circles has to do the
side question itself.

## Tested

Run with the real `claude` (2.1.295), in a scratch folder:

1. A first call remembered a made-up word.
2. A second call used `--resume <that chat> --fork-session --no-session-persistence --tools ""`
   and answered it correctly in about 4.6 s with Haiku, reading the first chat's context.
3. **`--no-session-persistence` works with a fork**: no new session file was written, so a side
   question leaves nothing in `~/.claude/projects`.
4. The cost was about $0.0026 for that call, because the context came from the prompt cache. A
   fork on the same model as the chat reads the same cache. Another model starts cold.
5. **A catch:** `--tools "Read,Grep,Glob"` limits Claude's built-in tools, but the call still
   listed the user's connected MCP servers (claude.ai connectors). A side question must also
   pass `--strict-mcp-config` with an empty config so it can reach nothing else.

## Design

- **A command** `session_side_question(session_id, question)`. It starts a separate, short Claude
  Code process with:
  `--resume <claude session id> --fork-session --no-session-persistence --output-format
  stream-json --include-partial-messages`, `--model <the chat's model>`, `--strict-mcp-config
  --mcp-config '{"mcpServers":{}}'`, and a short appended system prompt: "This is a side
  question. Answer briefly from the conversation. You cannot change anything."
- **Tools:** none by default. Setting "Let side questions read files" allows `Read,Grep,Glob`
  only, in the chat's folder, in `plan`-like read-only mode. Never Bash, Edit or Write.
- **Streaming:** the answer streams into the pane as it arrives, so it feels instant.
- **A chat that has not started yet** (no Claude session id) answers with no context and says so.
- **The working agent is untouched:** a separate process, a forked session, nothing written back.
- **Limits:** one side question at a time per chat, a 60 s timeout, a Stop button, a cap on answer
  size, and `Redactor` on the question.
- **Never a way around permissions:** it has no tools that change anything, so it cannot do what a
  chat would ask about.

## UI

- `/btw` shows in the `/` picker with the line "Ask a side question. The chat is not changed."
- Enter sends it to the pane instead of the chat. The message box stays as it was, with its draft.
- **The pane** sits directly above the message box, with the question, the streaming answer, a
  Stop button while it works, **Close** and **Add to chat** (puts the question and answer into the
  message box as a quote, for the person to send or edit).
- Esc closes it. It is one pane per chat. A new side question replaces the old one.
- The shortcut: ⌘⇧B opens it with the box focused.
- Works for workspace chats and for Composer runs (read-only, no tools, never starts anything).

## Phases

| Phase | What | Output |
|---|---|---|
| B0 | **Spike.** A throwaway test that forks a real chat and prints the streamed answer, with the exact flags above. Check: a chat mid-turn, a chat that was never run, a very long chat, the empty MCP config, time to the first word. | notes in this file |
| B1 | Backend: `smart`-style runner for the forked call, streaming events, Stop, timeout, size cap, redaction, a fake `claude` for tests | Rust |
| B2 | Frontend: the pane, `/btw` in the picker, the shortcut, streaming, Close, Add to chat | UI |
| B3 | Settings: model for side questions (the chat's, or Haiku), let them read files on or off; docs, hardening, manual guide | done |

## Risks

- **Cost:** each question spends tokens. On the chat's own model the cache makes it small, and
  Haiku is faster still. The pane shows nothing about cost, so the setting says so.
- **Context is only what is on disk.** The fork sees the conversation as saved. A turn in
  progress may not be in it yet, and the answer says "up to the last saved message".
- **A forked call can be slow on a huge chat.** The 60 s limit and Stop cover it.
- **Another process per question.** It starts and ends with the question, nothing stays running.
- **Claude Code changes its flags.** The runner is small and has one test that runs the real
  program when `CIRCLES_REAL_CLAUDE` is set.

## Decisions for you

1. Default model for a side question: the chat's own (cheap through the cache) or Haiku (fast)?
   Recommended: the chat's own.
2. Should side questions be able to read files, or answer only from the conversation?
   Recommended: only the conversation at first, with a setting.
3. "Add to chat": as a quote in the box (recommended), or add straight into the chat history?

## Decisions taken

1. **Model:** Haiku by default (your "small model"). A setting, "Model for side questions", can use
   the chat's own model instead, which can share its prompt cache.
2. **Files:** the question answers from the conversation only. A setting lets it read files in the
   chat's folder. It can never change anything.
3. **Add to chat:** puts the question and answer into the message box as a quote. Nothing is
   written into the chat history, and nothing is sent until you send it.

## Built (2026-10-11)

- **B0:** the command line was checked against the real `claude`: a fork of a saved chat answered
  from its context, with no tools, no connected servers and no hooks (`tools=[] mcp=[]`), and
  wrote no session file. The runner was run against it too: "kumquat" in 2.5 s.
- **B1:** `apps/desktop/src-tauri/src/side/` (`mod.rs` runner and parser, `service.rs` one question per chat) and
  `commands/side.rs`: `session_side_question`, `session_side_stop`, `side:event` events. Stop, a 60 s
  limit, an 8,000 character answer cap, the question redacted, a new question stops the old one.
- **B2:** `features/chat/side-pane.tsx`, `stores/side.ts`, `/btw` in the `/` picker, ⌘⇧B, the answer
  streams, Stop, Close (Esc too), Add to chat.
- **B3:** settings under Agents (model, read files), notes in `docs/hardening.md`, changelog.

## Gaps

- **A turn in progress** is not in the fork yet: the answer is from the conversation as saved.
- **A chat that never ran** has no context, and the pane says so.
- **One pane per chat.** A new question replaces the old one. There is no history of them.
- **Each question starts a Claude process** and spends tokens. Haiku keeps it small.
- **`--settings` with `disableAllHooks`** skips the person's own hooks for side questions, on
  purpose, so a hook cannot act on a throwaway call. It also means a hook-based setup does not apply.
- **Not tried by eye** in the running app, in both themes. The real program was run through the
  runner only, not through the app.
- **Composer runs:** works the same way, but a Composer's chat uses tools through a helper, and the
  fork has none, so a side question about what a tool "just did" answers from the saved text only.
