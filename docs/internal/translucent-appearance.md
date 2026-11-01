# A translucent appearance (Apple-style glass): plan

Status: built in phases 1 to 4. Checked by tests and builds; not yet seen in the running app. Circles has Light, Dark and System (`appearance.theme`). This adds a
**Translucency** option that gives macOS-style glass: the window's chrome shows a blurred view of what is
behind it, and floating surfaces are frosted. macOS first; Windows and Linux later.

## Decision: a material, not a fourth theme

Glass is a material, not a colour scheme, so it is a separate setting that works on top of Light, Dark and
System: `appearance.translucency` = `off` (default) or `on`. A fourth theme would need its own copy of
every colour and would not follow the system.

## Two layers

1. **Native window (macOS).** The window is transparent and has a vibrancy material behind the web view.
   This is what lets the sidebar show the wallpaper and other windows blurred, which CSS cannot do.
2. **CSS glass inside the window.** `backdrop-filter: blur() saturate()` with a translucent fill, a thin
   light edge and a soft shadow, for surfaces that float over Circles' own content: message box (already
   frosted), popovers, menus, dialogs, the Composer run panel, the pull request pane header.

Only the chrome is translucent: sidebar, top bar and the gaps. The chat, code, diffs and Monaco stay
opaque, so reading contrast and speed do not depend on the wallpaper.

## What it needs

- **Tauri window:** `transparent: true` and the `effects` window option (or the `window-vibrancy` crate) with
  a material such as `sidebar` or `underWindow`. `titleBarStyle: "Overlay"` is already set. Transparency on
  macOS needs the `macos-private-api` feature of Tauri, which rules out the Mac App Store (not planned).
  Turning translucency on or off at run time applies or clears the effect from Rust (a command), so no
  restart is needed. This must be checked in `pnpm tauri dev` and in a built app, not only in tests.
- **Tokens:** `packages/ui/src/styles/tokens.css` gets glass tokens (`--glass-fill`, `--glass-border`,
  `--glass-blur`, `--glass-shadow`) for light and dark. The `check:tokens` rule (no raw colours outside
  `tokens.css`) stays. Surfaces use a `data-translucent` attribute on `<html>`, like `data-density`.
- **Setting:** Settings > Appearance > Translucency, stored in the settings file like the others
  (`defs.ts`, `apply.ts`, the Rust settings struct, the generated types).
- **Fallbacks:** off when macOS "Reduce transparency" is on (`prefers-reduced-transparency`, and a native
  check for the window), and off on a platform without the effect.
- **Contrast:** `check:contrast` runs for the glass tokens against a light and a dark worst-case backdrop,
  so text on a translucent surface passes on a bright and a dark wallpaper.

## What changed while building

- **Liquid Glass is available.** The Tauri version in use (2.12) has `LiquidGlassRegular` for macOS 26 and
  newer, so the window asks for it, with the `Sidebar` material as the fallback for older macOS. The
  plan above called it an experiment; it turned out not to need `objc2`.
- **Everything is translucent.** The page background takes a glass tint too (`--glass-background-*`), so the
  chat, the top bar and the right pane show the window material, not only the sidebar and the floating
  surfaces. Code editors and terminals keep their own solid colours. This replaces the first build, which
  kept the centre column opaque.
- **Reduce transparency** is left to CSS (`prefers-reduced-transparency`), where the tints become the
  solid colours. The native material is also made solid by macOS itself.
- **Contrast floor:** the strengths had to be high to pass the check over a mid grey backdrop (light
  sidebar 0.92, light surfaces 0.86, light page 0.88; dark 0.74, 0.78 and 0.78), so the light theme shows less of the backdrop than
  the dark one. Lower them only with the check changed on purpose.
- **Not decided here:** the three questions below. Defaults used: off, one material (Liquid Glass or
  Sidebar), macOS only.

## Not in scope

- Interactive glass effects of macOS 27 (`interactive` in Tauri's effects) are not used.
- Windows (Mica/Acrylic) and Linux: the same setting, a different backend. Later.

## Risks

- **Legibility over a busy wallpaper.** Mitigated by opaque content areas and the contrast check.
- **Speed.** Blur over large areas costs GPU time. Floating surfaces only; no blur over the chat list or
  Monaco. Checked by hand on a long chat.
- **Private API.** `macos-private-api` is a documented Tauri feature but not public Apple API.
- **Cannot be tested here:** how it looks and feels. The PRs say what was and was not seen.

## Phases (stacked PRs)

1. **Setting and tokens.** `appearance.translucency`, the glass tokens, `data-translucent`, the contrast
   check. No visible change until a surface uses it.
2. **Native window.** `transparent`, the vibrancy material, the command to apply and clear it, Reduce
   transparency.
3. **Surfaces.** Sidebar, top bar, popovers, menus, dialogs, the run panel and the pull request header use
   the glass tokens when it is on.
4. **Docs.** `settings.md`, `CHANGELOG.md`, through `circles-update-docs`.

## Questions for you

1. **Default:** Off (recommended) until it has been checked on your Mac, then maybe On.
2. **Material:** `sidebar` for the sidebar and `underWindow` for the rest, or one material for the window?
3. **Windows and Linux:** wanted soon, or only when Circles ships there?

## Process

Jira is not set up, so each PR is a recorded "Ticket: None" exception.
