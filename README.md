# Circles

Run Claude Code agents in parallel, each in an isolated git worktree. Local-first desktop app built with Tauri 2.

<img width="1337" height="864" alt="image" src="https://github.com/user-attachments/assets/48a38932-baa2-40fd-9a30-997aa046be3e" />

Plan: [docs/phases/README.md](docs/phases/README.md)

## Install
Build the app with `scripts/release.sh` (run `pnpm model:fetch` first, once) and open the `.dmg`, or
run it from source with `pnpm tauri dev`.

The app is not signed yet, because there is no Apple Developer account. macOS will not open it with
a double click the first time: right-click the app, choose **Open**, and confirm. If macOS says it is
damaged, run `xattr -dr com.apple.quarantine /Applications/Circles.app` once. Details:
[docs/troubleshooting.md](docs/troubleshooting.md).

On first launch Circles shows a setup checklist: `git`, Claude Code (installed and signed in),
the GitHub CLI, notifications, the folders where your code lives, and the notice that agents run
with your permissions. Required items must be ready before the first project is added. Security and
audit notes: [docs/hardening.md](docs/hardening.md). Releasing:
[docs/release-checklist.md](docs/release-checklist.md). What macOS may ask, and when:
[docs/setup-audit.md](docs/setup-audit.md).

## Requirements
macOS, Node 24 (`.nvmrc`), pnpm, Rust stable, Xcode Command Line Tools, `git`, `gh`, `claude`.

## Layout
```
apps/desktop/        the app: src/ (React), src-tauri/ (Rust), vite.config.ts
packages/ui          @circles/ui: design tokens, Tailwind theme, shadcn components
packages/bindings    @circles/bindings: TypeScript types generated from Rust
packages/config      @circles/config: shared tsconfig presets
Cargo.toml           Rust workspace; the build output is target/ at the root
pnpm-workspace.yaml  apps/* and packages/*
scripts/  docs/      checks, release, documentation
```
Commands are run from the repository root. See [docs/phases/phase-20-monorepo.md](docs/phases/phase-20-monorepo.md).

## Commands
```bash
pnpm install        # install deps
pnpm tauri dev      # run the app with hot reload
pnpm typecheck      # TypeScript
pnpm lint           # Biome
pnpm check:rust     # cargo check
pnpm test           # frontend component tests (Vitest, fake backend)
pnpm test:rust      # Rust tests
pnpm gen:types      # regenerate TypeScript types from Rust
pnpm build          # production frontend build
scripts/test-all.sh # every check and test
scripts/release.sh  # test, then build the .dmg
```
