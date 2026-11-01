// Checks WCAG contrast for key token pairs in both themes. Usage: pnpm check:contrast
import { readFileSync } from "node:fs";

const css = readFileSync(new URL("../packages/ui/src/styles/tokens.css", import.meta.url), "utf8");

function block(selector) {
  const start = css.indexOf(`${selector} {`);
  const end = css.indexOf("\n}", start);
  const vars = {};
  const body = css.slice(start, end);
  for (const m of body.matchAll(/--([\w-]+):\s*(#[0-9a-fA-F]{6})\s*;/g)) {
    vars[m[1]] = m[2];
  }
  for (const m of body.matchAll(/--([\w-]+-alpha):\s*([\d.]+)\s*;/g)) {
    vars[m[1]] = Number(m[2]);
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

// Glass: a translucent surface sits over the blurred desktop, which can be anything. The window's
// material follows the theme, so text must pass over the extreme that theme can meet (white for light,
// black for dark) and over a mid grey, the worst for muted text.
function over(tint, alpha, backdrop) {
  const mix = [1, 3, 5].map((i) => {
    const t = Number.parseInt(tint.slice(i, i + 2), 16);
    const b = Number.parseInt(backdrop.slice(i, i + 2), 16);
    return Math.round(t * alpha + b * (1 - alpha));
  });
  return `#${mix.map((c) => c.toString(16).padStart(2, "0")).join("")}`;
}
const GLASS = [
  ["chrome", "sidebar-foreground", 4.5],
  ["chrome", "muted-foreground", 4.5],
  ["surface", "popover-foreground", 4.5],
  ["surface", "muted-foreground", 4.5],
  ["background", "foreground", 4.5],
  ["background", "muted-foreground", 4.5],
];
for (const [name, vars] of Object.entries(themes)) {
  for (const [kind, fg, min] of GLASS) {
    const extreme = name === "light" ? ["white", "#ffffff"] : ["black", "#000000"];
    for (const [label, backdrop] of [extreme, ["grey", "#808080"]]) {
      const surface = over(vars[`glass-${kind}-tint`], vars[`glass-${kind}-alpha`], backdrop);
      const r = ratio(vars[fg], surface);
      const ok = r >= min;
      if (!ok) failed++;
      console.log(
        `${ok ? "ok  " : "FAIL"} ${name.padEnd(5)} ${fg} on glass ${kind} over ${label}: ${r.toFixed(2)} (min ${min})`,
      );
    }
  }
}
if (failed) {
  console.error(`${failed} pair(s) below minimum`);
  process.exit(1);
}
