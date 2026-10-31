# Circles guide (website)

The user guide of Circles, built with [Fumadocs](https://www.fumadocs.dev) on Next.js and hosted on Vercel.

```
pnpm --filter @circles/docs dev      # http://localhost:3000
pnpm --filter @circles/docs check    # the pages: titles, links, text the build would drop
pnpm --filter @circles/docs test     # tests of that check
pnpm --filter @circles/docs build    # runs the check, then builds the site
```

## Writing a page

- Pages are Markdown in `content/docs/`. The order of the sidebar is `content/docs/meta.json`.
- A page needs `title` and `description` in its frontmatter. The build fails without a title.
- Link to another page as `./page.md` or `./page.md#heading`. Nothing else is found on the site.
  A file outside the guide is linked by its address on GitHub.
- Put `<owner>/<name>`, `{ticket}` and the like in backticks. Outside code they are read as a tag or an
  expression and the text is dropped without an error. `pnpm --filter @circles/docs check` finds them.

## Vercel

Connected by the owner in Vercel; nothing here deploys. The settings:

- **Root directory:** `apps/docs`, with "Include files outside the root directory" on (it is a pnpm workspace).
- **Install:** `pnpm install --frozen-lockfile`. **Build:** `pnpm build`. **Node:** 24.
- **Production branch:** `main`.
- **Ignored build step** (build only when the site changed): `git diff --quiet HEAD^ HEAD -- . ../../pnpm-lock.yaml`.
- **Environment:** `NEXT_PUBLIC_SITE_URL` when the site has a domain of its own, for the share cards.
  Without it, Vercel's production address is used.
