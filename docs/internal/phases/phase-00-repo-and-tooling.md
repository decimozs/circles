# Phase 0: Repo and tooling

## Goal
A running empty Tauri 2 app with the full toolchain, so every later phase starts from a clean, consistent base.

## Depends on
Nothing. `git init` is already done.

## In scope
- Scaffold Tauri 2 with React, TypeScript, Vite in the repo root.
- pnpm as the package manager.
- Tailwind CSS and shadcn/ui initialised.
- Biome (or ESLint + Prettier) for lint and format. Pick one and stick to it.
- Path alias `@/` for `src/`.
- `.gitignore`, `.editorconfig`, `.nvmrc`.
- Rust workspace layout under `apps/desktop/src-tauri/` with a `crates/` convention for later splitting.
- `README.md` with the dev commands.

## Out of scope
Any real UI, any backend logic, any commit or push.

## Deliverables
- `pnpm tauri dev` opens a window that renders a placeholder screen.
- `pnpm build`, `pnpm lint`, `pnpm typecheck`, `cargo check` all pass.
- Folder skeleton:
  ```
  src/            components/ features/ hooks/ lib/ routes/ stores/ styles/
  apps/desktop/src-tauri/      src/{commands,db,git,harness,pty,settings,workspace}/
  docs/phases/
  ```

## Technical notes
- Window: macOS overlay titlebar (`titleBarStyle: "Overlay"`, `hiddenTitle: true`), min size 900x600. Traffic lights are positioned in phase 2.
- Keep `tauri.conf.json` identifier as `build.circles.app` until a real domain is chosen.
- Do not add Tauri shell or fs plugin permissions beyond what a phase needs. Spawning goes through our own Rust commands.
- Add a `rust-toolchain.toml` pinning the toolchain.

## Done checklist
- [ ] App window opens on macOS
- [ ] All four checks pass
- [ ] Folder skeleton exists
- [ ] README explains setup in under ten lines
- [ ] `git status` shows untracked files only, no commits

## Manual test script
1. `pnpm install && pnpm tauri dev`
2. Window appears with placeholder text.
3. Edit a React file, confirm hot reload.
4. Edit a Rust file, confirm rebuild.
