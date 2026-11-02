import type { SkillGroup } from "@amplify/bindings/SkillGroup";
import type { SkillRow } from "@amplify/bindings/SkillRow";

export type SortId = "name" | "changed";

export const SORTS: { id: SortId; label: string }[] = [
  { id: "name", label: "Name" },
  { id: "changed", label: "Recently changed" },
];

export type ModelFilter = "any" | "yes" | "no";
export type Filters = {
  group: SkillGroup | "all";
  /** Whether the model may call the skill by itself. */
  model: ModelFilter;
  /** Only skills that have scripts or other files. */
  files: boolean;
};

export const NO_FILTERS: Filters = { group: "all", model: "any", files: false };

export function filtersSet(f: Filters): number {
  return Number(f.group !== "all") + Number(f.model !== "any") + Number(f.files);
}

export type Parsed = { words: string[]; group?: string; project?: string; source?: string };

/** `group:project`, `project:amplify` and `source:~/.claude` narrow the list, and the rest of the
 * words must each appear in the name or the description. */
export function parseQuery(query: string): Parsed {
  const parsed: Parsed = { words: [] };
  for (const token of query.trim().split(/\s+/).filter(Boolean)) {
    const m = /^(group|project|source):(.+)$/i.exec(token);
    if (!m) {
      parsed.words.push(token.toLowerCase());
      continue;
    }
    const key = m[1].toLowerCase() as "group" | "project" | "source";
    parsed[key] = m[2].toLowerCase();
  }
  return parsed;
}

export function matches(row: SkillRow, parsed: Parsed, filters: Filters): boolean {
  if (filters.group !== "all" && row.group !== filters.group) return false;
  if (filters.model === "yes" && !row.model_invocable) return false;
  if (filters.model === "no" && row.model_invocable) return false;
  if (filters.files && row.files === 0) return false;
  if (parsed.group && !row.group.startsWith(parsed.group)) return false;
  if (parsed.project && !row.source.toLowerCase().includes(parsed.project)) return false;
  if (parsed.project && row.group !== "project") return false;
  if (parsed.source && !row.source.toLowerCase().includes(parsed.source)) return false;
  const text = `${row.name} ${row.description}`.toLowerCase();
  return parsed.words.every((w) => text.includes(w));
}

export function sortRows(rows: SkillRow[], sort: SortId): SkillRow[] {
  const by =
    sort === "changed"
      ? (a: SkillRow, b: SkillRow) => b.modified_ms - a.modified_ms
      : (a: SkillRow, b: SkillRow) => a.name.localeCompare(b.name);
  return [...rows].sort((a, b) => by(a, b) || a.name.localeCompare(b.name));
}

export type Section = { id: string; title: string; rows: SkillRow[] };

const ORDER: Record<SkillGroup, number> = { yours: 0, shared: 1, plugin: 2, project: 3 };

/** Yours, Shared, Plugins, then one section per project, each with its skills in the order given. */
export function groupRows(rows: SkillRow[]): Section[] {
  const sections = new Map<string, Section>();
  for (const row of rows) {
    const id = row.group === "project" ? `project:${row.project_id ?? row.source}` : row.group;
    const title =
      row.group === "yours"
        ? "Yours"
        : row.group === "shared"
          ? "Shared"
          : row.group === "plugin"
            ? "Plugins"
            : row.source;
    const section = sections.get(id) ?? { id, title, rows: [] };
    section.rows.push(row);
    sections.set(id, section);
  }
  return [...sections.values()].sort((a, b) => {
    const ga = a.rows[0].group;
    const gb = b.rows[0].group;
    return ORDER[ga] - ORDER[gb] || a.title.localeCompare(b.title);
  });
}
