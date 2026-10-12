# Release checklist

1. `node scripts/bump-version.mjs X.Y.Z`, then `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml`.
2. Move the notes in `CHANGELOG.md` from "Unreleased" under the new version and date.
3. `scripts/test-all.sh` is green.
4. Optional, costs a few cents: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml -- --ignored` runs the tests that talk to the real `claude`.
5. `pnpm model:fetch` (once), then `scripts/release.sh` builds the `.dmg` (universal when both Rust targets are installed). It checks the title model first.
6. On a clean macOS user: install the `.dmg` (right-click, Open, while it is unsigned), run the Setup checklist, create a workspace, send a message. Write down every macOS prompt that appears and when (see `docs/setup-audit.md`).
7. Create five workspaces with running agents, force quit, relaunch, check they recover.
8. Keyboard-only pass through Home, Create, a workspace and Settings.
9. `scripts/scan-secrets.sh` reports nothing.
10. Signing and notarization: set `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD` and `APPLE_TEAM_ID`, then build again. Not possible without an Apple Developer account.
11. Tag the release: `git tag vX.Y.Z`.

Versions follow semantic versioning. While below 1.0, a minor bump may change behaviour.
