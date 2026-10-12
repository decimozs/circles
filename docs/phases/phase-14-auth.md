# Phase 14: Auth (Better Auth)

## Status
Deferred. The hosting decision is still open and must be made before this phase starts.

## Goal
Optional accounts using Better Auth, with sign-in by email and password, Google, GitHub, or Apple.

## Depends on
Phases 2 and 13 (UI shell and settings). Everything before this phase works without an account.

## Decision needed before starting
Where does Better Auth run? It is a TypeScript server library, so it needs an HTTP handler and a database.

| Option | Notes |
|---|---|
| A. Hosted service | Small Hono or Next service with Postgres or Turso. Google, GitHub, and Apple all work. Apple needs the paid Apple Developer account and an HTTPS redirect. Enables sync and teams later. |
| B. Local sidecar | Bundled Node or Bun sidecar with SQLite on localhost. Email and password, Google, GitHub work. Apple does not accept a localhost redirect, so it will not work. |

Also decide: does sign-in gate the app, or is it optional? Local features stay usable without sign-in by default.

## In scope
- Better Auth server with the email and password plugin and three social providers (Google, GitHub, Apple).
- Tauri client flow: system browser for OAuth, return via a `circles://auth/callback` deep link (`tauri-plugin-deep-link`), session stored in the macOS Keychain (not in plain files).
- Sign-in and sign-up screens in the app theme, forgot-password and email verification flows.
- Account settings page: profile, linked providers, sign out, delete account.
- Sidebar header and settings account page show the user, falling back to the local machine name when signed out.
- Session refresh and expiry handling, offline tolerance (the app never blocks local work on network).

## Out of scope
Sync of workspaces or chats, teams, billing.

## Deliverables
- Auth service (location depends on the decision), client integration, deep-link handler, Keychain storage, account UI.

## Technical notes
- Register the `circles` URL scheme in `tauri.conf.json`.
- Provider setup per environment: Google and GitHub OAuth apps, Apple Services ID and key.
- Use PKCE where supported. Validate state and nonce. Never put tokens in logs or URLs shown to the user.
- Rate-limit sign-in endpoints and require verified email for password accounts.

## Done checklist
- [ ] Email and password sign up, verify, sign in, reset
- [ ] Google, GitHub, and Apple sign-in complete via deep link
- [ ] Session survives restart through the Keychain
- [ ] Sign out clears the Keychain entry
- [ ] App is fully usable offline and signed out
- [ ] Account deletion works

## Manual test script
1. Sign up with email, verify, sign out, sign in.
2. Sign in with each provider, confirm the deep link returns to the app.
3. Quit and relaunch, confirm the session persists.
4. Disconnect the network, confirm local features still work.
5. Delete the account, confirm the session is gone.
