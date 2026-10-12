import { cn } from "@circles/ui/utils";
import {
  DatabaseIcon,
  FileIcon,
  FileImageIcon,
  FileTextIcon,
  FolderIcon,
  FolderOpenIcon,
  KeyRoundIcon,
  LockIcon,
} from "lucide-react";
import type { IconType } from "react-icons";
import {
  SiBiome,
  SiC,
  SiCplusplus,
  SiCss,
  SiDart,
  SiDocker,
  SiElixir,
  SiGit,
  SiGnubash,
  SiGo,
  SiGraphql,
  SiHtml5,
  SiJavascript,
  SiJson,
  SiKotlin,
  SiLua,
  SiMarkdown,
  SiNpm,
  SiOpenjdk,
  SiPhp,
  SiPnpm,
  SiPrisma,
  SiPython,
  SiReact,
  SiRuby,
  SiRust,
  SiSass,
  SiSharp,
  SiSqlite,
  SiSvelte,
  SiSvg,
  SiSwift,
  SiTailwindcss,
  SiToml,
  SiTypescript,
  SiVite,
  SiVuedotjs,
  SiYaml,
} from "react-icons/si";

type Glyph = { icon: IconType | typeof FileIcon; tone: string };

const info = "text-info";
const warning = "text-warning";
const success = "text-success";
const bad = "text-destructive";
const quiet = "text-muted-foreground";

const BY_EXTENSION: Record<string, Glyph> = {
  ts: { icon: SiTypescript, tone: info },
  mts: { icon: SiTypescript, tone: info },
  cts: { icon: SiTypescript, tone: info },
  tsx: { icon: SiReact, tone: info },
  jsx: { icon: SiReact, tone: info },
  js: { icon: SiJavascript, tone: warning },
  mjs: { icon: SiJavascript, tone: warning },
  cjs: { icon: SiJavascript, tone: warning },
  json: { icon: SiJson, tone: warning },
  jsonc: { icon: SiJson, tone: warning },
  rs: { icon: SiRust, tone: warning },
  py: { icon: SiPython, tone: info },
  go: { icon: SiGo, tone: info },
  java: { icon: SiOpenjdk, tone: bad },
  kt: { icon: SiKotlin, tone: warning },
  swift: { icon: SiSwift, tone: warning },
  c: { icon: SiC, tone: info },
  h: { icon: SiC, tone: info },
  cpp: { icon: SiCplusplus, tone: info },
  hpp: { icon: SiCplusplus, tone: info },
  cs: { icon: SiSharp, tone: success },
  rb: { icon: SiRuby, tone: bad },
  php: { icon: SiPhp, tone: info },
  lua: { icon: SiLua, tone: info },
  dart: { icon: SiDart, tone: info },
  ex: { icon: SiElixir, tone: info },
  exs: { icon: SiElixir, tone: info },
  sh: { icon: SiGnubash, tone: success },
  bash: { icon: SiGnubash, tone: success },
  zsh: { icon: SiGnubash, tone: success },
  css: { icon: SiCss, tone: info },
  scss: { icon: SiSass, tone: bad },
  html: { icon: SiHtml5, tone: warning },
  vue: { icon: SiVuedotjs, tone: success },
  svelte: { icon: SiSvelte, tone: bad },
  graphql: { icon: SiGraphql, tone: bad },
  gql: { icon: SiGraphql, tone: bad },
  prisma: { icon: SiPrisma, tone: quiet },
  md: { icon: SiMarkdown, tone: quiet },
  markdown: { icon: SiMarkdown, tone: quiet },
  mdx: { icon: SiMarkdown, tone: quiet },
  yml: { icon: SiYaml, tone: bad },
  yaml: { icon: SiYaml, tone: bad },
  toml: { icon: SiToml, tone: bad },
  svg: { icon: SiSvg, tone: warning },
  sql: { icon: DatabaseIcon, tone: info },
  sqlite: { icon: SiSqlite, tone: info },
  db: { icon: DatabaseIcon, tone: info },
  png: { icon: FileImageIcon, tone: success },
  jpg: { icon: FileImageIcon, tone: success },
  jpeg: { icon: FileImageIcon, tone: success },
  gif: { icon: FileImageIcon, tone: success },
  webp: { icon: FileImageIcon, tone: success },
  ico: { icon: FileImageIcon, tone: success },
  bmp: { icon: FileImageIcon, tone: success },
  lock: { icon: LockIcon, tone: quiet },
  env: { icon: KeyRoundIcon, tone: warning },
  txt: { icon: FileTextIcon, tone: quiet },
};

/** Files known by their whole name, which say more than their extension. */
const BY_NAME: Record<string, Glyph> = {
  "package.json": { icon: SiNpm, tone: bad },
  "pnpm-lock.yaml": { icon: SiPnpm, tone: warning },
  "pnpm-workspace.yaml": { icon: SiPnpm, tone: warning },
  "biome.json": { icon: SiBiome, tone: info },
  dockerfile: { icon: SiDocker, tone: info },
  ".dockerignore": { icon: SiDocker, tone: info },
  ".gitignore": { icon: SiGit, tone: bad },
  ".gitattributes": { icon: SiGit, tone: bad },
  ".gitmodules": { icon: SiGit, tone: bad },
  "vite.config.ts": { icon: SiVite, tone: warning },
  "vite.config.js": { icon: SiVite, tone: warning },
  "tailwind.config.js": { icon: SiTailwindcss, tone: info },
  "tailwind.config.ts": { icon: SiTailwindcss, tone: info },
  "cargo.lock": { icon: LockIcon, tone: quiet },
};

export function glyphFor(name: string): Glyph {
  const lower = name.toLowerCase();
  const byName = BY_NAME[lower];
  if (byName) return byName;
  if (lower.startsWith(".env")) return { icon: KeyRoundIcon, tone: warning };
  const ext = lower.includes(".") ? (lower.split(".").pop() ?? "") : "";
  return BY_EXTENSION[ext] ?? { icon: FileIcon, tone: quiet };
}

export function FileIconFor({
  name,
  isDir,
  open,
  className,
}: {
  name: string;
  isDir: boolean;
  open?: boolean;
  className?: string;
}) {
  if (isDir) {
    const Folder = open ? FolderOpenIcon : FolderIcon;
    return <Folder className={className} />;
  }
  const { icon: Icon, tone } = glyphFor(name);
  return <Icon className={cn(className, tone)} data-icon={Icon.name} />;
}
