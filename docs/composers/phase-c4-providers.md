# Phase C4: Providers and keys

> **Deferred.** Composers use Claude only for now. Nothing here is needed for C2, C3 or C5 to C7. Come back to it if other models are wanted. No provider library is chosen: the first step would be to compare thin clients over `reqwest` with an existing multi-provider crate.

## Goal
A Composer can run on OpenAI, Grok or Anthropic through their APIs, using the registry directly.

## Depends on
C2.

## In scope
- **Provider trait:** one interface for "send messages and tool definitions, get text and tool calls", with streaming. Implementations for Anthropic, OpenAI and xAI (OpenAI-compatible).
- **Direct loop:** the Rust run engine calls the provider, runs tools through the registry (no MCP), stores steps, and continues. It uses the same resume rules as C2.
- **Keys:** stored in the macOS Keychain, one per provider. The new Composers page in Settings (under Agents & environment) shows whether a key is set, never its value. A test button checks a key.
- **Model choice per Composer:** provider and model, with a short list of known models and a free text field.
- **Cost:** tokens and an estimated cost for each step and each run, with a spend limit per run and per day. Reaching a limit stops the run.
- **Failures:** rate limits and network errors retry with backoff, then pause the run as interrupted so it can be resumed.

## Out of scope
Local models, fine-tuning, embedding or search features.

## Deliverables
- `apps/desktop/src-tauri/src/providers/`, a Keychain wrapper, settings page for keys, cost display.

## Technical notes
- Keys are read when a run starts and held in memory only. They are added to the log redactor.
- Tool-call formats differ by provider. Each implementation converts to and from one internal step format.
- Tests use a fake provider that returns scripted tool calls. Real provider tests are opt-in and cheap.

## Done checklist
- [ ] The same Composer works on at least two providers
- [ ] Keys never appear in settings, logs or the database
- [ ] Spend limits stop a run
- [ ] A rate limit pauses and resumes cleanly
- [ ] A run switched to another provider keeps its history

## Manual test script
1. Add an OpenAI key, test it, and run a small Composer on it.
2. Switch the Composer to another provider and repeat.
3. Set a tiny spend limit and confirm the run stops.
4. Search the logs and the database for the key.
