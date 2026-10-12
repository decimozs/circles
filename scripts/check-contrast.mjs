// Checks WCAG contrast for key token pairs in both themes. Usage: pnpm check:contrast
import { readFileSync } from "node:fs";

const css = readFileSync(new URL("../packages/ui/src/styles/tokens.css", import.meta.url), "utf8");

function block(selector) {
  const start = css.indexOf(`${selector} {`);
  const end = css.indexOf("\n}", start);
  const vars = {};
  for (const m of css.slice(start, end).matchAll(/--([\w-]+):\s*(#[0-9a-fA-F]{6})\s*;/g)) {
    vars[m[1]] = m[2];
  }
  return vars;
}

const themes = { light: block(":root"), dark: { ...block(":root"), ...block(".dark") } };

function luminance(hex) {
  const [r, g, b] = [1, 3, 5].map((i) => {
    const c = Number.parseInt(hex.slice(i, i + 2), 16) / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function ratio(a, b) {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

// [foreground, background, minimum ratio]
const PAIRS = [
  ["foreground", "background", 4.5],
  ["foreground", "sidebar", 4.5],
  ["card-foreground", "card", 4.5],
  ["popover-foreground", "popover", 4.5],
  ["muted-foreground", "background", 4.5],
  ["muted-foreground", "muted", 4.5],
  ["muted-foreground", "sidebar", 4.5],
  ["secondary-foreground", "secondary", 4.5],
  ["accent-foreground", "accent", 4.5],
  ["primary-foreground", "primary", 4.5],
  ["destructive", "background", 4.5],
  ["success", "background", 4.5],
  ["warning", "background", 4.5],
  ["info", "background", 4.5],
  ["violet", "background", 4.5],
  ["diff-add", "diff-add-bg", 4.5],
  ["diff-remove", "diff-remove-bg", 4.5],
  ["terminal-foreground", "terminal-background", 4.5],
  ["ring", "background", 3],
  ["border", "background", 1.15],
];

let failed = 0;
for (const [name, vars] of Object.entries(themes)) {
  for (const [fg, bg, min] of PAIRS) {
    const r = ratio(vars[fg], vars[bg]);
    const ok = r >= min;
    if (!ok) failed++;
    console.log(
      `${ok ? "ok  " : "FAIL"} ${name.padEnd(5)} ${fg} on ${bg}: ${r.toFixed(2)} (min ${min})`,
    );
  }
}
if (failed) {
  console.error(`${failed} pair(s) below minimum`);
  process.exit(1);
}
