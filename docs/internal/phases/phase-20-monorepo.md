# Phase 20: monorepo

Status: steps 0 to 4 are done (baseline, workspace shell, desktop app moved to `apps/desktop`,
`@circles/bindings`, `@circles/ui`), with the shared config (`@circles/config`, the pnpm catalog,
import boundaries, lefthook and CI). `apps/web` and `apps/mobile` are not created: they wait for the
open questions below.

Findings while doing steps 3 and 4:

- Biome turns its React rules on from the `react` dependency in the config's own `package.json`.
  The root no longer lists React, so `linter.domains.react` is set explicitly. Without it the
  `useExhaustiveDependencies` rule silently stopped running and every suppression looked unused.
- A nested `biome.json` replaces `files.includes` for its folder, which un-ignored the generated
  bindings. The import boundaries are `overrides` in the root config instead.
- `check:tokens` greps both the app and `packages/ui/src`. Its path pattern is checked by planting a
  raw colour: it fails when it should.
- Tailwind scans the shared components through one `@source` line in the app's `globals.css`. The
  built CSS is the same size as before (108.00 kB).

Findings while doing step 2, now fixed:

- `[profile.release]` in a member crate is ignored by a workspace. It moved to the root
  `Cargo.toml`, otherwise release builds would have lost LTO, `panic = "abort"` and `strip`.
- A moved `target` keeps stale absolute paths in the llama.cpp CMake cache. The llama crates were
  cleaned once (`cargo clean -p llama-cpp-sys-2 -p llama-cpp-2`) and rebuilt, which is the only
  thing that was recompiled from scratch.
- `tauri build` sets `MACOSX_DEPLOYMENT_TARGET` to 10.13 by default, and llama.cpp needs 10.15.
  A production build was failing before the move as well. `bundle.macOS.minimumSystemVersion` is
  now `11.0`.

## Why

Circles is one Tauri app today: React in `src/`, Rust in `src-tauri/`. A landing page and a mobile
app are coming. They should share what is worth sharing (design tokens, UI components, the types the
Rust side generates) without the desktop app being tangled into them, and without any feature of the
desktop app changing.

The rule for the whole phase: **the desktop app behaves exactly as before after every step.** Every
step ends with the full suite green, and a step that cannot is not done.

## What is true today (facts the plan rests on)

- One Rust crate `circles` (lib `circles_lib` + bin) in `src-tauri/`. `src-tauri/target` is about
  65 GB, mostly a compiled llama.cpp, so moving it carelessly costs a long rebuild.
- ts-rs writes the TypeScript types to `src/lib/bindings` (157 files). The target comes from
  `.cargo/config.toml` (`TS_RS_EXPORT_DIR`). 69 frontend files import them as `@/lib/bindings/...`.
- 62 files import `@/components/ui/...` (shadcn on Base UI) and 42 import `@/lib/utils`.
- `@/` is an alias for `src/`, set in `tsconfig.json`, `vite.config.ts` (also used by vitest) and
  `components.json`.
- `src/styles/tokens.css` is the only place colours may be written. `pnpm check:tokens` greps `src`
  for raw colours, and `scripts/check-contrast.mjs` reads `tokens.css`.
- `tauri.conf.json` runs `pnpm dev` / `pnpm build` and takes `../dist`.
- Rust tests use `CARGO_MANIFEST_DIR` for `target/debug/circles` (`composer/launch.rs`,
  `composer/server.rs`) and for the model (`smart/embedded.rs`).
- Scripts with paths in them: `fetch-model.sh`, `check-model.sh`, `test-all.sh`, `release.sh`,
  `bump-version.mjs`; `biome.json` ignores `src-tauri/*`, `src/components/ui`, `src/lib/bindings`.
- The repository has **no commits yet**.

## Target layout

```
circles/
  apps/
    desktop/            the app as it is: index.html, vite.config.ts, src/, src-tauri/
    web/                the landing page (new)
    mobile/             later
  packages/
    ui/                 @circles/ui: tokens.css, base styles, shadcn components, cn()
    bindings/           @circles/bindings: the generated types (ts-rs output)
  docs/  scripts/
  Cargo.toml            Rust workspace: members = ["apps/desktop/src-tauri"]
  pnpm-workspace.yaml   packages: apps/*, packages/*
  biome.json  .cargo/  rust-toolchain.toml  .editorconfig  .nvmrc
```

Not now, only when a second consumer exists:

- **`crates/circles-core`** (the Rust logic split out of the Tauri crate). A mobile or CLI client
  would need it. Splitting it before then is risk with no user.
- **Turborepo.** Plain `pnpm -r` scripts are enough for three apps. Add it when builds get slow.
- **A shared `packages/app` with screens.** Desktop screens depend on Tauri IPC; the web and mobile
  apps would not reuse them.

## Configuration, file by file

Everything below is decided up front so no step has to invent a convention.

### Names

| What | Name | Why |
|---|---|---|
| Root package | `circles-monorepo`, `private: true` | Never published, never imported |
| Apps | `@circles/desktop`, `@circles/web`, `@circles/mobile` | One scope, easy `pnpm --filter` |
| Packages | `@circles/ui`, `@circles/bindings`, `@circles/config` | Same scope. `config` holds shared tsconfig presets |
| Rust package | `circles` (unchanged), lib `circles_lib` (unchanged) | The MCP helper (`--circles-mcp`) and tests find the binary as `circles`. Renaming breaks both |
| Future Rust crate | `circles-core` | Only when a second consumer exists |
| Tauri `identifier`, product name | unchanged | Keeps the data folder, Keychain entries and notification permission |

Every app and package implements the same script names (`dev`, `build`, `typecheck`, `lint`,
`test`), so `pnpm -r <script>` works across all of them.

### pnpm

- `pnpm-workspace.yaml`: `packages: [apps/*, packages/*]`, and a `catalog:` for the versions that must
  not drift (react, react-dom, typescript, vite, vitest, tailwindcss, @biomejs/biome, @types/react).
  Apps and packages write `"react": "catalog:"`.
- Internal dependencies use `workspace:*`.
- Root `package.json`: `packageManager` pinned to the current pnpm, `engines.node` equal to `.nvmrc`
  (24). `.npmrc`: `shared-workspace-lockfile=true`, `strict-peer-dependencies=false` only if a peer
  warning cannot be fixed.
- Packages are consumed as source (`exports` point at `src/*.ts(x)`), with no build step. Vite and
  vitest transform them. That keeps one TypeScript program and no stale `dist` to forget.

### TypeScript

- `packages/config/tsconfig.base.json`: today's flags (`strict`, `noUnusedLocals`,
  `noUnusedParameters`, `noFallthroughCasesInSwitch`, `isolatedModules`, `moduleResolution: bundler`,
  `noEmit`, `skipLibCheck`).
- `packages/config/tsconfig.react.json` extends the base: `jsx: react-jsx`, `lib` with DOM.
- `packages/config/tsconfig.node.json` for config files (`vite.config.ts`).
- Each app and package has a small `tsconfig.json` that `extends` one of those and sets only `include`
  and `paths`. `apps/desktop` keeps `"@/*": ["./src/*"]`, so none of its 600 internal imports change.
- Typecheck is per package (`tsc --noEmit`), run together by `pnpm -r typecheck`. No project
  references are needed because nothing is built.

### Biome (linter and formatter)

- One root `biome.json`, with today's rules and formatter (2 spaces, width 100, recommended preset).
  A single config is what keeps every package formatted the same.
- `files.includes` becomes: `**`, `!**/target`, `!**/gen`, `!**/dist`, `!packages/bindings/src`
  (generated), `!packages/ui/src/components/ui` (generated by shadcn).
- Per-app differences, if ever needed, go in a nested `biome.json` with `"extends": "//"`.
- `.editorconfig` is unchanged and applies to the whole tree (Rust 4 spaces, the rest 2).
- Import boundaries (step 5): a Biome `noRestrictedImports` override for `apps/web` and
  `apps/mobile`, and for `packages/*`, forbidding `@circles/desktop`, `@tauri-apps/*` (in `ui` and
  `bindings`) and relative imports out of the package.

### Tests

- Each package runs its own vitest (`vitest run`), with its own `environment` and setup. Desktop keeps
  `jsdom`, `src/test/setup.ts` and its worker limit. A package of plain types needs no tests.
- Root `pnpm test` is `pnpm -r test` plus `cargo test`. The accessibility tests (axe) stay with the
  screens that own them.

### Styling

- `packages/ui/src/styles/tokens.css` is the only place a colour is written. `check:tokens` and
  `check:contrast` move with it and fail loudly when the path is wrong.
- Apps import `@circles/ui/styles.css`, and Tailwind v4 finds the package's classes with an `@source`
  line. Fonts (`@fontsource-variable/*`) stay in the app, so the web page can choose its own.

### Rust

- Root `Cargo.toml`: `[workspace] resolver = "2"`, `members = ["apps/desktop/src-tauri"]`, plus
  `[workspace.package]` (edition, authors, version) and `[workspace.dependencies]` for crates that
  more than one member will use. `Cargo.lock` moves to the root.
- Target directory: the workspace root's `target/`, which is where the 65 GB is moved.
- `.cargo/config.toml` at the root: `TS_RS_EXPORT_DIR` becomes `packages/bindings/src`. Because the
  path is relative to the config's own folder, it works from any member.
- `rust-toolchain.toml`: stay on `stable`, but add `components = ["rustfmt", "clippy"]` so a fresh
  checkout has them. Pinning an exact version is a separate decision.
- `rustfmt` and clippy keep their defaults. `clippy --all-targets -- -D warnings` stays the gate.
- `tauri.conf.json` stays inside `apps/desktop/src-tauri`: `frontendDist: "../dist"` still means
  `apps/desktop/dist`, and the `pnpm dev` / `pnpm build` hooks run in `apps/desktop`. Checked by
  running `tauri dev` and `tauri build --debug` in step 2.
- The three tests that read `CARGO_MANIFEST_DIR/target/...` use `CARGO_TARGET_DIR` with a fallback
  to the workspace root.

### Git and hooks

- `.gitignore`: `**/node_modules`, `**/dist`, `target`, `**/gen`, and the model path under
  `apps/desktop/src-tauri/resources/models/*.gguf`.
- `lefthook.yml` today is only the example file. Make it real: pre-commit runs `biome check` on
  staged files and `cargo fmt --check` on staged `.rs`; pre-push runs `pnpm -r typecheck` and the
  affected tests. It runs nothing heavy on commit.

### Scripts and release

- Root scripts: `dev` (desktop), `dev:web`, `build`, `test`, `lint`, `format`, `typecheck`, `check`
  (tokens and contrast), `gen:types`, `model:fetch`, `model:check`.
- `scripts/test-all.sh` runs both languages from the root, with the Rust commands run as
  `cargo ... --workspace`.
- `bump-version.mjs` updates the root, `apps/desktop/package.json`, `tauri.conf.json` and the
  Rust `[workspace.package]` version, so one number stays true.
- `release.sh` looks for the `.dmg` in the workspace's `target`.

### CI

- GitHub Actions with path filters: a change under `apps/web` or `packages/ui` runs the frontend
  jobs; a change under `src-tauri` runs the Rust job; the root config runs both.
- The Rust job caches `target` (and `~/.cargo`), runs on macOS, and skips the model download unless a
  test needs it. The cost is the llama.cpp compile, so the cache key includes `Cargo.lock`.

### Documentation

- `README.md`: a layout section and the new commands. `CONTRIBUTING.md`: how to add an app and a
  package. `docs/release-checklist.md`, `docs/setup-audit.md`, `docs/hardening.md`: paths updated.
  `CHANGELOG.md` stays one file at the root.

## What is shared, and what is not

| Shared | Why |
|---|---|
| `tokens.css`, `globals.css` base, contrast script | One look across all apps |
| `components/ui/*`, `cn()` | The same controls on web and desktop |
| Generated Rust types | One source of truth for anything that talks to the Rust side |

| Stays in `apps/desktop` | Why |
|---|---|
| `features/*`, `stores/*`, `ipc.ts`, router, Tauri plugins | Tied to the desktop's IPC and screens |
| All Rust | The app's engine |

## Steps

Each step is its own commit, with all checks green, and can be undone alone.

0. **Baseline commit.** Commit the repository as it is (check the author identity first, as always)
   so every later move is a `git mv` that can be reverted. Run and record: `pnpm test`,
   `pnpm typecheck`, `pnpm lint`, `pnpm check:tokens`, `pnpm check:contrast`, `cargo test`,
   `cargo clippy --all-targets`. These are the numbers every step must match: 535 frontend,
   843 Rust tests.
1. **Workspace shell, no moves.** Add `pnpm-workspace.yaml` and a root Cargo workspace that lists
   `src-tauri`. Nothing moves. `pnpm install` and `cargo test` still pass. This proves the tooling
   before any path changes.
2. **Move the desktop app.** `git mv` `src`, `src-tauri`, `index.html`, `vite.config.ts`,
   `tsconfig*.json`, `components.json`, `public` into `apps/desktop/`. Then, in this order:
   - `mv src-tauri/target target` to the repository root and set the Cargo target to it, so the
     compiled llama.cpp is reused (only the `circles` crate itself rebuilds).
   - `.cargo/config.toml`: `TS_RS_EXPORT_DIR` relative to the new place.
   - `tauri.conf.json`: `beforeDevCommand: pnpm --filter desktop dev`, `frontendDist` adjusted.
   - The three Rust tests that read `CARGO_MANIFEST_DIR/target/...` use `CARGO_TARGET_DIR` or the
     workspace root instead.
   - `package.json` scripts, `biome.json` ignores, `scripts/*`, `docs/*` paths.
   Gate: the whole suite, plus `pnpm tauri dev` opens and runs a chat.
3. **Extract `@circles/bindings`.** Point ts-rs at `packages/bindings/src`, add an `index`
   re-export, and rewrite the 69 imports from `@/lib/bindings/X` to `@circles/bindings/X` with one
   codemod. Gate: typecheck and tests.
4. **Extract `@circles/ui`.** Move `components/ui`, `lib/utils.ts`, `styles/tokens.css` and
   `styles/globals.css` into `packages/ui`. Rewrite the 62 + 42 imports with a codemod. Keep Tailwind
   scanning the package (`@source`), keep `components.json` pointing at the package so `shadcn add`
   still works, and keep `check:tokens` and `check:contrast` running over the package. Gate:
   typecheck, tests, token and contrast checks, and a build that has the same CSS size.
5. **Add `apps/web`** (landing page) consuming `@circles/ui`. It must not import anything from
   `apps/desktop`. Add a lint rule (Biome `noRestrictedImports`, or a dependency check script) that
   fails when one app imports another.
6. **Root scripts and CI.** `pnpm -r test|typecheck|lint`, one `scripts/test-all.sh` for both
   languages, and a CI workflow that runs it, with path filters so a landing page change does not run
   the Rust suite.
7. **`apps/mobile`** when its shape is known (see the questions below).

## What could break, and how it is held back

| Risk | Held back by |
|---|---|
| A broken `@/` import after a move | Step 2 keeps `@/` inside `apps/desktop` unchanged. Only steps 3 and 4 change imports, by codemod, with typecheck as the gate |
| Types stop generating | `gen:types` runs in steps 2 and 3, and the number of generated files (157) is compared |
| 65 GB target rebuilt | Move `target`, do not recreate it. Only the app crate recompiles |
| Tests that find the binary or model by path | The three `CARGO_MANIFEST_DIR` uses are fixed in step 2 and covered by running the full suite |
| Tauri dev and build stop finding the frontend | Checked by hand in step 2: `tauri dev`, and `tauri build --debug` |
| Colour rule bypassed once tokens move | `check:tokens` and `check:contrast` are pointed at `packages/ui` in step 4, and fail if the path is missing |
| Updater, signing, bundle identifier | None changes. `identifier` and the product name stay. Data in `~/Library/Application Support/circles` is untouched |
| Shared package pulls Tauri code into the web app | `packages/ui` may not depend on `@tauri-apps/*`. The import-boundary rule in step 5 enforces it |
| `shadcn add` writes to the wrong place | `components.json` aliases point at `packages/ui`, tested once by adding and removing a component |

## Open questions (they change steps 5 to 7, not 0 to 4)

1. **What is mobile?** A remote control for the desktop app (needs the desktop to expose a secure
   local or relayed API, which Circles does not have: it is local-only by design), or a standalone
   app? Stack: Expo / React Native, or Tauri mobile? This decides whether `circles-core` is ever
   extracted.
2. **Landing page stack:** Astro, Next, or Vite + React? Astro is the lightest and still consumes
   `@circles/ui` through React islands.
3. **Versioning:** one version for everything (current `bump-version.mjs`), or per app?

## Recommendation

Do steps 0 to 2 first. They are mechanical, they unblock everything, and nothing is shared yet, so
nothing can drift. Do 3 and 4 right after, while there is no history to rewrite. Hold 5 to 7 until
the questions above are answered.
