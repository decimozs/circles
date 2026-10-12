#!/usr/bin/env bash
# Runs every check and test, locally, in the order a release needs them.
# Usage: scripts/test-all.sh
set -euo pipefail
cd "$(dirname "$0")/.."

step() { printf '\n==> %s\n' "$1"; }

step "Biome"
pnpm lint
step "TypeScript"
pnpm typecheck
step "Colour tokens and contrast"
pnpm check:tokens
pnpm check:contrast
step "Frontend tests (includes accessibility checks)"
pnpm test
step "Frontend build"
pnpm build
step "Rust format"
cargo fmt --all --check
step "Rust lints"
cargo clippy --workspace --all-targets -- -D warnings
step "Rust tests"
cargo test --workspace

printf '\nAll checks passed.\n'
