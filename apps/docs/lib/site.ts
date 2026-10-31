/** The name of the site, and the address it is served from. */
export const siteName = "Circles";
export const siteDescription =
  "Circles runs Claude Code agents in parallel on your Mac, each in its own copy of your repository.";

type Env = Record<string, string | undefined>;

/**
 * The address the site is served from, for the absolute links in share cards. It is set with
 * `NEXT_PUBLIC_SITE_URL` when there is a domain of its own; otherwise Vercel's production address
 * is used, and on a developer's machine `localhost`.
 */
export function siteUrl(env: Env = process.env): URL {
  const own = env.NEXT_PUBLIC_SITE_URL?.trim();
  if (own) return new URL(own.startsWith("http") ? own : `https://${own}`);
  const vercel = env.VERCEL_PROJECT_PRODUCTION_URL?.trim();
  if (vercel) return new URL(`https://${vercel}`);
  return new URL("http://localhost:3000");
}
