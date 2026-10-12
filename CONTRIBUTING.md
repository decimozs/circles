# Contributing

- Plan and scope per phase: `docs/phases/`.
- `scripts/test-all.sh` runs every check. Run it before sending a change.
- Colours live only in `apps/desktop/src/styles/tokens.css` (`pnpm check:tokens`).
- Rust types shared with the UI are generated: `pnpm gen:types`, never edited by hand.
- Tests never talk to the real `claude`. The few that do are `#[ignore]`d and opt in.
- Commit messages and pull requests carry no tool attribution lines.

## Monorepo

- Apps live in `apps/*`, shared code in `packages/*`, all named `@circles/<name>`.
- A shared package must not import an app or `@tauri-apps/*`, and an app must not import another app.
  Biome enforces both.
- `@circles/ui`: after `shadcn add` (run it in `packages/ui`), run `pnpm --filter @circles/ui fix:imports`
  so the new component uses relative imports.
- Versions of React, TypeScript, Vite, Vitest, Tailwind and Biome are in the `catalog:` of
  `pnpm-workspace.yaml`. Write `"react": "catalog:"` in a new package, not a version.
- A new app: add `apps/<name>/package.json` named `@circles/<name>` with the scripts `dev`, `build`,
  `typecheck`, `test`, extend `@circles/config/tsconfig.react.json`, and import the theme with
  `@import "@circles/ui/styles/theme.css"` and an `@source` line for `packages/ui/src`.
