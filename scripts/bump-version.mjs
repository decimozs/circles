// Sets the version in every place it lives. Usage: node scripts/bump-version.mjs 0.2.0
import { readFileSync, writeFileSync } from "node:fs";

const version = process.argv[2];
if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(version ?? "")) {
  console.error("Usage: node scripts/bump-version.mjs <major.minor.patch>");
  process.exit(1);
}

const edit = (file, change) => {
  const path = new URL(`../${file}`, import.meta.url);
  writeFileSync(path, change(readFileSync(path, "utf8")));
};

edit("package.json", (t) => t.replace(/"version": "[^"]*"/, `"version": "${version}"`));
edit("apps/desktop/package.json", (t) =>
  t.replace(/"version": "[^"]*"/, `"version": "${version}"`),
);
edit("apps/desktop/src-tauri/tauri.conf.json", (t) =>
  t.replace(/"version": "[^"]*"/, `"version": "${version}"`),
);
edit("apps/desktop/src-tauri/Cargo.toml", (t) =>
  t.replace(/^version = "[^"]*"/m, `version = "${version}"`),
);
console.log(
  `Version set to ${version}. Run \`cargo check\` to update Cargo.lock, then edit CHANGELOG.md.`,
);
