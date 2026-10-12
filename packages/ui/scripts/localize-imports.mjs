// `shadcn add` writes imports through the `@/` alias, which means something else inside the app that
// uses this package. Run this after adding a component to make them relative.
import { readdirSync, readFileSync, writeFileSync } from "node:fs";

const dir = new URL("../src/components/", import.meta.url);
for (const name of readdirSync(dir)) {
  if (!name.endsWith(".tsx")) continue;
  const file = new URL(name, dir);
  const before = readFileSync(file, "utf8");
  const after = before
    .replace(/from "@\/components\/([\w-]+)"/g, 'from "./$1"')
    .replace(/from "@\/utils"/g, 'from "../utils"');
  if (after !== before) {
    writeFileSync(file, after);
    console.log(`fixed imports in ${name}`);
  }
}
