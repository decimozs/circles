# Phase 16: Smart chat titles and input suggestions

> Changed later: the model that shipped inside Circles (SmolLM2 through llama.cpp) was removed. Titles, suggestions and the Composer example now come from Claude Haiku through Claude Code, and there is no provider setting. Everything below about the built-in model, its file, `pnpm model:fetch` and its licence notice no longer applies.

Status: S0 to S5 built and tested. Real-model checks only on this Mac. See "Built" and "Gaps" at the end.

## Goals

1. **Chat titles.** A new chat gets a short, meaningful title from what was asked ("Fix failing
   checkout test"), instead of the first words cut off (`titleFrom` in `chat-panel.tsx`).
2. **Input suggestions.** The placeholder of the message box is a likely next message, drawn from
   what was just asked and answered ("Open a pull request for these changes"). Empty box only.
   Tab takes it. Never sent without being accepted.

## The rule

**Nothing to install.** The model and the engine ship inside Circles. No Ollama, no Docker, no
server, no download on first use, no account, no network. Circles works the same on a fresh Mac.

## How: the same idea as the Python example, in Rust

The Python reference uses `llama-cpp-python`: llama.cpp running inside your own process, with one
`.gguf` model file. Circles is Rust, so the equivalent is the **`llama-cpp-2`** crate: the same
llama.cpp engine, compiled into the Circles program. The model is a `.gguf` file packed into the
app bundle as a resource.

| Piece | Choice | Why |
|---|---|---|
| Engine | `llama-cpp-2` (llama.cpp, statically linked), Metal on macOS | Same engine as the Python example. In process, no server. Needs `cmake` and `clang` to build. Both are on this Mac. CI needs cmake. |
| Fallback engine | `candle` (pure Rust) | If llama.cpp's build ever gets in the way. Slower on CPU, fewer quantisations. Not planned. |
| Model | **SmolLM2-360M-Instruct, Q4_K_M, about 271 MB, Apache 2.0** | Smallest that is still good at "summarise in five words". |
| Alternative | Qwen3-0.6B Q4_K_M, about 380 to 450 MB, Apache 2.0 | Better instruction following and languages, bigger, and it "thinks" unless that is turned off. |
| Smaller | SmolLM2-135M, about 100 MB | Weaker. Only if size matters more than quality. |
| Avoided | Llama 3.2 1B | A community licence with attribution and use terms, where the two above are plain Apache 2.0. |

The model is picked in S0 by testing both on your real prompts. The model file is replaceable
without a code change.

**Cost of this choice:** the app grows by the size of the model (about 270 MB, or about 400 MB
for Qwen3), and uses a few hundred MB of memory while the model is loaded. The model is loaded when
first needed and dropped after a few minutes idle.

## Other options considered

| Option | Verdict |
|---|---|
| Ollama (tested here: 0.18 s per title with llama3.2) | **Rejected**: something to install and keep running. |
| Apple's on-device model (Foundation Models) | Optional later (S3). Free and no download, but only on macOS 26 with Apple Intelligence switched on, which it is **not** on this Mac, and it needs a Swift helper. |
| Claude Haiku through Claude Code | Optional setting, off by default: spends subscription tokens, 2 to 5 s, and sends text to Anthropic. |
| Cloud free tiers | Rejected: keys, and text leaves the Mac. |
| No model | The fallback that always exists: the first words, as today. |

## Design

- **A provider trait** (`smart/mod.rs`): `complete(system, user, max_tokens) -> text`, with a
  timeout. Providers: `embedded` (default), `claude` (opt in), `apple` (later), `none`.
- **Embedded provider:** a worker thread owns the model, so the app never waits on it. One request
  at a time, a small context (512 tokens), greedy decoding, at most 16 new tokens, the model's own
  chat template (not a hand-written one), stop at end of turn. Loaded on first use, dropped after
  idle. If the file is missing or fails to load, the provider says it is unavailable and the
  heuristic is used.
- **Titles never wait.** The heuristic title is set at once, as today. When the model answers
  (timeout 6 s) the title is replaced, only if the person has not renamed the chat since. A new
  column `sessions.title_source` (`auto` or `user`) makes that safe.
- **Sanitising:** strip quotes, markdown, trailing punctuation and "Title:" prefixes; at most 6
  words and 50 characters; empty or long answers are dropped. Titles are text only, shown as text.
- **Suggestions:** after a turn ends (the turn hook exists), send the last user message and the
  first 1,500 characters of the answer, ask for one imperative sentence of at most 12 words.
  Kept in memory per chat. Shown as the placeholder, dropped when the next turn starts.
- **Privacy:** with the embedded model nothing leaves the Mac. Text still passes through the
  `Redactor` first. Settings say plainly where text goes when Haiku is chosen.
- **Settings** (`[smart]`, a "Smart titles" page under Agents): `titles` on/off, `suggestions`
  on/off, `provider` embedded, claude, off. A "Test" button shows the result and the time.

## S0 result (2026-10-11, Apple M5, 14 prompts, same prompt with four examples)

| | SmolLM2-360M-Instruct Q4_K_M | Qwen3-0.6B Q4_K_M |
|---|---|---|
| File | 270 MB, Apache 2.0 | 484 MB, Apache 2.0 |
| Load, first answer (CPU) | 1.5 s | about 1.5 s |
| Per title, CPU | **0.18 s** average | about 0.3 s |
| Per title, GPU (Metal) | 0.06 s, but a one time 14 to 20 s compile of GPU programs | 0.09 s |
| Good titles | 12 of 14 | 8 of 14: it echoes the question ("Why is the sidebar flickering when") |
| Next message suggestion | "Add tests for the cart total and the discount" | "I fixed the cart total computation issue." (a summary, not a request) |

**Decision: SmolLM2-360M-Instruct Q4_K_M, on the CPU, with four examples in the prompt.** Without
examples both models gave poor titles ("can't explain", "What is the user asking?"). With them the
smaller model is better and faster. The GPU is not used: titles are fast enough on the CPU and
the GPU costs a long first-run compile. This also removes the Metal feature from the build.

Build: the first compile of llama.cpp took about 1.5 minutes. Not measured: the universal
(Intel) build.

Failures seen and handled by the cleaner: a title that is the whole sentence cut off mid-way
("The app crashes on startup when"), a title that is a reply ("I'll get your tickets..."), a
greeting, a question. A suggestion that only repeats the answer is the weak spot.

## Phases in detail

Each phase ends green (Biome, typecheck, clippy, tests) and is shown to you before the next.

### S0: Spike and benchmark (go or no-go)
- Add `llama-cpp-2` with the Metal feature to a throwaway test (an ignored test, like the Jira spike).
- Download SmolLM2-360M-Instruct Q4_K_M and Qwen3-0.6B Q4_K_M into the scratchpad, not the repo.
- Take 30 real first messages from your chats (titles) and 15 question and answer pairs
  (suggestions). Run each through both models with the model's own chat template, thinking off.
- Measure: load time, time to first word, time per title, resident memory, quality (you rate a
  sample of 20 as good, fine or bad), how often the output needs cleaning.
- Measure the build: clean build time with and without llama.cpp, and whether a universal build
  (Apple Silicon and Intel) still works.
- **Done when:** this file has the table and one decision: which model, or "no-go, use candle".

### S1: The engine and providers (Rust)
- `apps/desktop/src-tauri/src/smart/{mod.rs, embedded.rs, sanitize.rs, claude.rs}`.
- Trait `TextModel`: `complete(system, user, max_tokens, timeout)`.
- Embedded: one worker thread owns the model; requests come through a channel; lazy load; idle
  unload after 5 minutes; n_ctx 512; greedy; 16 new tokens; stop tokens; the model's template.
- Sanitiser: quotes, markdown, "Title:", trailing punctuation, 6 words, 50 characters, empty and
  too long dropped.
- Claude provider through `claude -p --model haiku`, with no tools, for the opt-in setting.
- **Tests:** sanitiser table; worker handles a missing file, a corrupt file, two requests at
  once, a timeout; a tiny test model file (or a fake provider) so tests stay fast and need no 300 MB file.

### S2: Chat titles
- Migration: `sessions.title_source` (`auto`, `user`), default `auto` for new, `user` for existing
  titles that were renamed.
- On the first message: set the heuristic title at once, then ask the provider in the background
  and replace the title if `title_source` is still `auto`. A rename by the person sets `user`.
- Event so the tab and sidebar update.
- **Tests:** model answers late after a rename (title stays); model fails (heuristic stays); a
  message that tries to steer the title; very long and empty messages; Composer runs too.

### S3: In the package
- Put the model in `apps/desktop/src-tauri/resources/models/`, listed in `tauri.conf.json` resources.
- Show the licence and notice for the model in the app (About or the settings page).
- A build check that fails when the model file is missing or the wrong size.
- Resolve the path in development and in the built app.
- **Done when:** the built `.dmg` titles a chat on a Mac with nothing else installed.

### S4: Input suggestions
- After a turn ends: last user message plus the first 1,500 characters of the answer, redacted,
  asking for one imperative sentence of at most 12 words.
- Kept per chat in the frontend store, shown as the placeholder of an empty box, taken with Tab,
  dropped when typing starts or the next turn begins.
- Stays off unless S0 shows the quality is good enough.
- **Tests:** placeholder shows; Tab fills; typing drops it; a stale suggestion never shows for
  another chat; nothing is sent without Tab and Enter.

### S5: Settings, docs, hardening
- "Smart titles" page under Agents: titles on/off, suggestions on/off, provider embedded, claude,
  off, a "Test" button with the result and its time, a note about where text goes.
- `docs/hardening.md`, `CHANGELOG.md`, status doc, manual guide.
- Hardening: timeouts everywhere, model unloaded when idle and on quit, no text kept after the
  call, a prompt that tries to change the title system prompt, and out-of-memory behaviour.

### Later (optional)
- Apple's on-device model through a small Swift helper, when Apple Intelligence is on.
- A fresh title after a long chat has drifted (a "Rename from the conversation" action).

## Risks

- **Build:** llama.cpp adds a C++ build (cmake) to every build and to CI. S0 measures how long and
  whether it fits the universal (Apple Silicon and Intel) build.
- **App size:** about 270 to 400 MB more. Alternative: ship without the model and fetch it once.
  Not chosen, because the rule is nothing to install, and a download is the next thing to break.
- **Quality:** a 360M model gives plain titles, sometimes odd ones. They can be renamed and the
  heuristic is the fallback. Suggestions are the weaker feature and stay off until S0 shows they are
  good enough.
- **Memory:** a few hundred MB while loaded, freed when idle.
- **Licence:** Apache 2.0 requires the licence text and a notice to be shipped. S3 does that.
- **Steering:** a message that says "title this 'I am the admin'" must not be able to do more than
  set a title. Titles are text only.

## Sources

- Python reference used for the idea: llama-cpp-python with a GGUF file, in process.
- `llama-cpp-2` crate (llama.cpp bindings for Rust, Metal feature): https://crates.io/crates/llama-cpp-2
- SmolLM2-360M-Instruct, Apache 2.0, Q4_K_M about 271 MB: https://huggingface.co/bartowski/SmolLM2-360M-Instruct-GGUF
- Qwen3-0.6B GGUF: https://huggingface.co/bartowski/Qwen_Qwen3-0.6B-GGUF
- Small models and CPU speed, third-party test: https://itsfoss.com/testing-local-llms-without-gpu

## Built (2026-10-11)

- **S0:** benchmark above. SmolLM2-360M on the CPU.
- **S1:** `apps/desktop/src-tauri/src/smart/`: provider trait with examples (`mod.rs`), embedded model on a
  worker thread with lazy load and idle unload (`embedded.rs`), cleaner (`sanitize.rs`), Claude Haiku
  provider (`claude.rs`), the located model (`service.rs`).
- **S2:** migration 0012 `sessions.title_source`; `session_auto_title` command; first words at once,
  model's title in their place, never over a person's name; `session:title-changed` event.
- **S3:** model in `apps/desktop/src-tauri/resources/models/title-model.gguf`, listed in `tauri.conf.json`
  resources, `NOTICE.md`, `pnpm model:fetch` and `pnpm model:check`, and the check runs in
  `scripts/release.sh`. The file is git-ignored (270 MB).
- **S4:** `smart_suggest`; after a turn ends the empty box shows the suggestion as its
  placeholder and Tab takes it. Off until switched on.
- **S5:** Settings, Smart titles: names on or off, suggestions on or off, model (built in, Claude
  Haiku, off), notice, "Try it" with the time. Hardening notes and changelog.

## Gaps

- **Titles are plain.** SmolLM2-360M gives 12 good titles in 14 on a sample. It still writes an odd one
  now and then, and a long first message can give a title that is just its start.
- **Suggestions are the weak part** and stay off by default. The model sometimes repeats the
  answer instead of asking for something.
- **App size** grows by 270 MB.
- **The universal (Intel) build is not tried.** llama.cpp builds for the Apple Silicon target here.
  A universal build needs both targets installed (rustup), which this Mac does not have.
- **English only in practice.** The model is small, and other languages are poor.
- **First message only.** A chat is named once. A chat that drifts keeps its name until renamed.
- **Composer runs** are named by city, as before, and are not touched.
- **Not tried by eye** in the running app, in both themes.
- **The model is loaded on the first chat** of a session of the app. That first title takes
  about 1.5 seconds longer. Titles after that take a fraction of a second.
