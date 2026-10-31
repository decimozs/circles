# A documentation website with Fumadocs

Status: phases 1 to 4 are built. Phase 5 (connect Vercel, add the address to the README) is for the owner.

The user guide moves to a website built with [Fumadocs](https://www.fumadocs.dev/docs), in `apps/docs`.
The website is where the guide lives and is read. On GitHub the repository's README gives the address of the
site instead of the pages. The `docs/` folder stays at the root for what still needs it: `docs/internal/`
(plans, audits, the threat model, the release checklist), which is for people working on Circles and stays
out of the site, and a short `docs/README.md` that says where the guide is.

## What Fumadocs needs (checked on its site)

- Node 22 or newer. This repository requires Node 24.
- A framework: Next.js, Waku, React Router, TanStack Start or Astro. Next.js is the one it is built around.
- Created with `npm create fumadocs-app` (or pnpm). Pages live in `content/docs/`. The source is set with
  `defineDocs({ dir: "content/docs" })` in `source.config.ts` and a `loader` in `lib/source.ts`.
- Its MDX collection handles `.md` and `.mdx`. A page has a `title` in its frontmatter.

### What the spike found (Fumadocs 16.17.9, fumadocs-mdx 15.4.10, Next.js 16.4.2)

Run with `apps/docs` and a build of probe pages, not read from the docs.

- **`title` is required.** A page with no frontmatter fails the build (`invalid frontmatter ... title:
  expected string`). The move adds frontmatter to every page.
- **A `.md` file is read as MDX-like, not as plain Markdown.** `<owner>/<name>` outside code was swallowed as an
  unknown tag and rendered as just `/`, with no error, and `<b>` was kept as HTML. Text is lost without a
  warning, so the move checks every page for `<...>` outside code. Checked now: none of the twelve guide pages
  has one (nor a bare `{` or `}`) outside code. The check stays in the migration step and in CI.
- **Links between pages resolve only as `./page.md`** (and `./page.md#anchor`), to `/docs/page`. A bare
  `page.md`, `page.md#anchor` or `../other.md` is left as it is and is broken on the site. The guide writes
  `page.md`, so the move rewrites each one to `./page.md`. Links to files outside the guide
  (`CONTRIBUTING.md`, `.github/JIRA.md`) are not resolved and become GitHub addresses.
- **The content source is in `lib/source.ts`** (`defineDocs` from `fumadocs-mdx/macro`), with the pages in
  `content/docs/` and the order in `content/docs/meta.json` (`"pages": [...]`). The previous and next links at
  the foot of a page follow that order. `dir` pointing outside the app was not tested: the content is moved
  into the app now, so it is not needed.
- **It brings more than pages:** search (`/api/search`), `llms.txt` and `llms-full.txt`, a Markdown copy of each
  page, and generated share images. They come with the template and are kept, except that the share
  images need a decision in phase 3.
- **pnpm 12 blocks install scripts by default.** `esbuild`, which the docs site pulls in, was reported as an
  ignored build and failed `pnpm install`. It does not need its script, so `allowBuilds: esbuild: false` is
  in `pnpm-workspace.yaml`.
- **Versions:** the template asks for TypeScript 7 and its own React and Tailwind versions. The app uses the
  repository's `catalog:` versions instead (TypeScript 6, React 19.1, Tailwind 4.3) and builds with them.

## Where things end up

| Now | After |
|---|---|
| `docs/README.md`, `getting-started.md`, `projects.md`, `workspaces.md`, `chats.md`, `pull-requests.md`, `composers.md`, `jira.md`, `usage.md`, `settings.md`, `shortcuts.md`, `troubleshooting.md` | `apps/docs/content/docs/` (moved with `git mv`, so their history follows). `README.md` becomes the introduction (`index`). |
| `docs/internal/` | stays |
| new `docs/README.md` | three lines: the guide is at the site's address, the plans and audits are in `internal/`. |
| the repository `README.md` | a link to the site where it links to `docs/README.md` and `docs/troubleshooting.md` now. The links to `docs/internal/` stay. |

The reading order in the old `docs/README.md` becomes the sidebar order in `meta.json`.

## The app

- `apps/docs`, package `@circles/docs`, Next.js with Fumadocs UI and Fumadocs MDX, in the pnpm workspace
  (`apps/*` is already in it), using the `catalog:` versions for React, TypeScript, Tailwind and Biome so they
  stay the same everywhere.
- Hosted on Vercel, which builds Next.js natively, so no static export is needed and Fumadocs' server
  features stay available. (Static export remains possible later if the host changes.)
- Search: Fumadocs' built-in search, which needs no service.
- It does not use `packages/ui` (the desktop app's design system) and `check:tokens` and `check:contrast`
  stay scoped to the desktop app.
- Look: Fumadocs' default theme first. Circles' colours and the Inter and JetBrains Mono fonts can be set
  from the same tokens afterwards.
- Because the pages are now written in the app, Fumadocs' components (callouts, tabs, steps) can be used in
  the text where they help. They are not added to the existing pages during the move.

## What has to change with it

Found by searching the repository for references to the guide:

- **`README.md`**: the "Guide" line and the troubleshooting line point to the site.
- **`CLAUDE.md`, `AGENTS.md`**: the layout line (`docs/` is the user guide) and rule 11.
- **`CONTRIBUTING.md`**: where documentation goes.
- **`.claude/skills/circles-update-docs/SKILL.md`**: the table that maps a change to a page now names
  `apps/docs/content/docs/<page>`, and its checks run the site's build, which also checks the links.
  `circles-atomic-commit` and the ticket spike template mention `docs/` and are checked for what they mean.
- **`CHANGELOG.md`** stays where it is.
- **Nothing in the app's code** points at the guide (searched).
- **CI**: a docs filter in the existing changed-paths job and a job that builds the site when
  `apps/docs/**` changes. `pnpm -r typecheck`, `pnpm -r test` and `pnpm lint` at the root include the new app.
  Its generated folders (`.next`, `out`) go in `.gitignore` and the Biome ignore list.

## Phases (stacked PRs)

1. **A spike (built).** `apps/docs` is created with `create-fumadocs-app` and shows one real page
   (Getting started). The open questions are answered above.
2. **Move the guide.** The twelve pages are moved in, with the frontmatter, the links and the sidebar order,
   and the site builds with every link checked. A page-by-page read for wording that only made sense on GitHub.
3. **Search, theme, fonts, the home page and the site's name.**
4. **CI, the skill and the repository's own docs** (the list above), done in one pull request so the
   repository never points at a place the guide is not.
5. **Hosting on Vercel and the address.** Vercel builds from the repository when the project is
   connected, so nothing in the code deploys. What is done is settings, listed below, and the README's link to
   the site once there is an address. Until then the README says how to run the site (`pnpm --filter
   @circles/docs dev`) and the guide stays readable from the repository. Connecting the project is done by
   the owner in Vercel; I do not deploy or connect it.

### Vercel settings (phase 5)

- **Root directory:** `apps/docs`, with "include files outside the root directory" on, since the install
  is a pnpm workspace at the repository root.
- **Install command:** `pnpm install --frozen-lockfile` (Vercel reads `packageManager` from the root
  `package.json`, which says pnpm 12.8.1). **Build command:** `pnpm build` (`next build`). **Node:** 24.
- **Production branch:** `main`. Every pull request gets a preview, which for a private repository sits
  behind Vercel's deployment protection by default, so only people with access see it.
- **Ignored build step:** build only when `apps/docs` changed
  (`git diff --quiet HEAD^ HEAD -- apps/docs ../../pnpm-lock.yaml`), so a change to the desktop app does not
  rebuild the site.
- **Plan:** the free Hobby plan covers a personal account and is for non-commercial use; a team or
  commercial project needs Pro. That is for the owner to judge.

Phase 4 and 5 are merged together or phase 5 first, so that on `main` the guide is never in neither place.

## Risks

- **Fumadocs moves fast.** Its major versions change configuration files. The version is pinned and the
  spike records which one this was checked against.
- **A second toolchain.** Next.js is large next to a Vite app. It installs for the docs app only and CI builds
  it only when docs change, but `pnpm install` grows.
- **The guide was written for GitHub.** Relative links, "this page" and tables are checked in phase 2.
- **Docs and code in one pull request.** A change to the app that needs a docs change now touches
  `apps/docs/`, in the same pull request, as the rule already says.
- **Contributors cannot read the guide in GitHub's file view any more.** That is the point, and the README
  says where it is.

## Questions for you

1. **At what address?** Hosted on Vercel (decided). The address Vercel gives, or a domain of the owner's,
   goes in the README.
2. **Should `docs/internal/` stay out of the site?** Recommended: yes. They are working plans and audits.
3. **Next.js, or another of Fumadocs' frameworks?** Recommended: Next.js, the best supported.
4. **A name for the site** (the title in its header).
