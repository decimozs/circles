// Checks the pages of the guide before the site is built, for the mistakes the build does not
// report. Found by building probe pages: a page with no title fails the build, but text such as
// `<owner>/<name>` outside code is swallowed without a word, and a link written `page.md` or with
// `../` is left broken on the site.
import { readdirSync, readFileSync } from "node:fs";
import { basename, join } from "node:path";
import { fileURLToPath } from "node:url";
import GithubSlugger from "github-slugger";

const FENCE = /^(\s*)(```|~~~)/;

/** The lines of a page with code taken out, so what is left is what a reader sees as text. Line
 * numbers stay the same: removed text is replaced by nothing on the same line. */
export function proseLines(text) {
  const out = [];
  let fence = null;
  for (const line of text.split("\n")) {
    const m = line.match(FENCE);
    if (fence) {
      if (m && m[2] === fence) fence = null;
      out.push("");
    } else if (m) {
      fence = m[2];
      out.push("");
    } else {
      out.push(line.replace(/`[^`\n]*`/g, ""));
    }
  }
  return out;
}

/** The frontmatter and where the body starts (a line number, from 1). */
export function frontmatter(text) {
  const m = text.match(/^---\n([\s\S]*?)\n---\n/);
  if (!m) return { data: {}, bodyLine: 1 };
  const data = {};
  for (const line of m[1].split("\n")) {
    const kv = line.match(/^([A-Za-z_-]+):\s*(.*)$/);
    if (kv) data[kv[1]] = kv[2].replace(/^"(.*)"$/, "$1");
  }
  return { data, bodyLine: m[0].split("\n").length };
}

/** The anchors a page offers: the slug of each of its headings, as the site makes them. */
export function anchors(text) {
  const slugger = new GithubSlugger();
  return new Set(
    proseLines(text)
      .map((l) => l.match(/^#{1,6}\s+(.*?)\s*#*$/))
      .filter(Boolean)
      .map((m) => slugger.slug(m[1].replace(/[`*_]/g, ""))),
  );
}

/** Every problem in the pages of `dir`, as `file:line: what is wrong`. */
export function checkContent(dir) {
  const problems = [];
  const files = readdirSync(dir).filter((f) => /\.mdx?$/.test(f));
  const pages = new Map(files.map((f) => [f.replace(/\.mdx?$/, ""), join(dir, f)]));
  const texts = new Map([...pages].map(([name, path]) => [name, readFileSync(path, "utf8")]));
  const at = (name, line, message) =>
    problems.push(`${basename(pages.get(name))}:${line}: ${message}`);

  for (const [name, text] of texts) {
    const { data, bodyLine } = frontmatter(text);
    if (!data.title) at(name, 1, "no title in the frontmatter (the build fails without one)");
    if (!data.description) at(name, 1, "no description in the frontmatter");
    proseLines(text).forEach((line, i) => {
      if (i + 1 < bodyLine) return;
      if (/<[A-Za-z/!]/.test(line)) {
        at(
          name,
          i + 1,
          "a `<` outside code is read as a tag and the text is lost: put it in backticks",
        );
      }
      if (/[{}]/.test(line)) {
        at(name, i + 1, "a `{` or `}` outside code is read as an expression: put it in backticks");
      }
      for (const m of line.matchAll(/\]\(([^)\s]+)\)/g)) {
        const target = m[1];
        if (/^(https?:|mailto:|#)/.test(target)) {
          if (target.startsWith("#") && !anchors(text).has(target.slice(1))) {
            at(name, i + 1, `there is no heading for ${target}`);
          }
          continue;
        }
        if (target.startsWith("/docs")) {
          const page = target.replace(/^\/docs\/?/, "").split("#")[0] || "index";
          if (!pages.has(page)) at(name, i + 1, `${target} is not a page`);
          continue;
        }
        if (!target.startsWith("./")) {
          at(
            name,
            i + 1,
            `write the link to ${target} as ./${target}: only ./page.md is found on the site`,
          );
          continue;
        }
        const [file, anchor] = target.split("#");
        const page = file.replace(/^\.\//, "").replace(/\.mdx?$/, "");
        if (!pages.has(page)) {
          at(name, i + 1, `${target} is not a page of the guide`);
        } else if (anchor && !anchors(texts.get(page)).has(anchor)) {
          at(name, i + 1, `${file} has no heading for #${anchor}`);
        }
      }
    });
  }

  // The order of the sidebar names real pages, and every page is in it.
  let meta = { pages: [] };
  try {
    meta = JSON.parse(readFileSync(join(dir, "meta.json"), "utf8"));
  } catch {
    problems.push("meta.json: missing or not valid JSON");
  }
  for (const p of meta.pages ?? []) {
    if (!p.startsWith("---") && !pages.has(p)) problems.push(`meta.json: "${p}" is not a page`);
  }
  for (const name of pages.keys()) {
    if (!(meta.pages ?? []).includes(name))
      problems.push(`meta.json: "${name}" is not in the order`);
  }
  return problems;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const dir = fileURLToPath(new URL("../content/docs", import.meta.url));
  const problems = checkContent(dir);
  for (const p of problems) console.error(p);
  if (problems.length > 0) {
    console.error(`${problems.length} problem${problems.length === 1 ? "" : "s"} in the guide.`);
    process.exit(1);
  }
  console.log("The guide's pages are fine.");
}
