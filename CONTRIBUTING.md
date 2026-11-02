# Contributing

- Plan and scope per phase: `docs/internal/phases/`.
- The user guide is a website in `apps/docs` (`pnpm --filter @amplify/docs dev`). Its pages are Markdown in
  `apps/docs/content/docs/`; `pnpm --filter @amplify/docs check` reports mistakes the build does not.
- `scripts/test-all.sh` runs every check. Run it before sending a change.
- Colours live only in `apps/desktop/src/styles/tokens.css` (`pnpm check:tokens`).
- Rust types shared with the UI are generated: `pnpm gen:types`, never edited by hand.
- Tests never talk to the real `claude`. The few that do are `#[ignore]`d and opt in.
- Commit messages and pull requests carry no tool attribution lines.

## Monorepo

- Apps live in `apps/*`, shared code in `packages/*`, all named `@amplify/<name>`.
- A shared package must not import an app or `@tauri-apps/*`, and an app must not import another app.
  Biome enforces both.
- `@amplify/ui`: after `shadcn add` (run it in `packages/ui`), run `pnpm --filter @amplify/ui fix:imports`
  so the new component uses relative imports.
- Versions of React, TypeScript, Vite, Vitest, Tailwind and Biome are in the `catalog:` of
  `pnpm-workspace.yaml`. Write `"react": "catalog:"` in a new package, not a version.
- A new app: add `apps/<name>/package.json` named `@amplify/<name>` with the scripts `dev`, `build`,
  `typecheck`, `test`, extend `@amplify/config/tsconfig.react.json`, and import the theme with
  `@import "@amplify/ui/styles/theme.css"` and an `@source` line for `packages/ui/src`.
- Vulnerabilities in dependencies: `pnpm audit --prod` and `cargo audit` (the `Audit` workflow runs both every
  week). Monaco ships its own copy of DOMPurify, so `patches/monaco-editor@*.patch` replaces it with a fixed
  one; a test fails if it stops applying, and the patch is dropped when a Monaco release has a fixed copy.
