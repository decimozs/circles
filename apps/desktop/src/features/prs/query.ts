import type { PrRow } from "@circles/bindings/PrRow";

export type SortId = "updated" | "newest" | "oldest" | "largest" | "smallest";

export const SORTS: { id: SortId; label: string }[] = [
  { id: "updated", label: "Recently updated" },
  { id: "newest", label: "Newest" },
  { id: "oldest", label: "Oldest" },
  { id: "largest", label: "Largest change" },
  { id: "smallest", label: "Smallest change" },
];

export type DraftFilter = "any" | "hide" | "only";
export type ChecksFilter = "any" | "passing" | "failing" | "pending";
export type ReviewFilter = "any" | "approved" | "changes_requested" | "review_required";

export type Filters = {
  draft: DraftFilter;
  checks: ChecksFilter;
  review: ReviewFilter;
};

export const NO_FILTERS: Filters = { draft: "any", checks: "any", review: "any" };

/** How many of the filters are set, for the badge on the Filters button. */
export function filtersSet(filters: Filters): number {
  return (
    Number(filters.draft !== "any") +
    Number(filters.checks !== "any") +
    Number(filters.review !== "any")
  );
}

export type Query = {
  /** Words that must be in the title, branch, author or repository. */
  words: string[];
  numbers: number[];
  label: string[];
  author: string[];
  repo: string[];
  branch: string[];
  /** `is:draft`, `is:open`, `is:merged`, `is:closed`, `is:mine`. */
  is: string[];
  review: string[];
  checks: string[];
};

/** Splits what was typed into words and `key:value` terms. Quotes keep a value together:
 * `label:"good first issue"`. */
export function parseQuery(text: string): Query {
  const query: Query = {
    words: [],
    numbers: [],
    label: [],
    author: [],
    repo: [],
    branch: [],
    is: [],
    review: [],
    checks: [],
  };
  const keyed: Record<string, string[] | undefined> = {
    label: query.label,
    author: query.author,
    repo: query.repo,
    branch: query.branch,
    is: query.is,
    review: query.review,
    checks: query.checks,
  };
  const tokens = text.match(/[^\s"]+:"[^"]*"|"[^"]*"|\S+/g) ?? [];
  for (const raw of tokens) {
    const token = raw.replace(/"/g, "").trim();
    if (!token) continue;
    const colon = token.indexOf(":");
    const target = colon > 0 ? keyed[token.slice(0, colon).toLowerCase()] : undefined;
    if (target && colon < token.length - 1) {
      target.push(token.slice(colon + 1).toLowerCase());
    } else if (/^#\d+$/.test(token)) {
      query.numbers.push(Number(token.slice(1)));
    } else {
      query.words.push(token.toLowerCase());
    }
  }
  return query;
}

const contains = (list: string[], value: string) =>
  list.some((v) => value.toLowerCase().includes(v));

/** Whether a row satisfies what was typed and the filters. Every term must hold. */
export function matches(row: PrRow, query: Query, filters: Filters): boolean {
  if (filters.draft === "hide" && row.is_draft) return false;
  if (filters.draft === "only" && !row.is_draft) return false;
  if (filters.checks !== "any" && row.checks.state !== filters.checks) return false;
  if (filters.review !== "any" && row.review !== filters.review) return false;

  if (query.numbers.length > 0 && !query.numbers.includes(row.number)) return false;
  for (const word of query.words) {
    const haystack = `${row.title} ${row.head_ref} ${row.author} ${row.repo} #${row.number}`;
    if (!haystack.toLowerCase().includes(word)) return false;
  }
  if (
    query.label.length > 0 &&
    !query.label.every((l) => row.labels.some((x) => x.name.toLowerCase().includes(l)))
  ) {
    return false;
  }
  if (query.author.length > 0 && !contains(query.author, row.author)) return false;
  if (query.repo.length > 0 && !contains(query.repo, `${row.repo} ${row.project_name}`))
    return false;
  if (query.branch.length > 0 && !contains(query.branch, row.head_ref)) return false;
  for (const value of query.is) {
    const ok =
      value === "draft"
        ? row.is_draft
        : value === "mine"
          ? row.by_me
          : value === "open" || value === "merged" || value === "closed"
            ? row.state === value
            : true;
    if (!ok) return false;
  }
  if (
    query.review.length > 0 &&
    !query.review.includes(row.review.replace(/_/g, " ")) &&
    !query.review.includes(row.review)
  ) {
    return false;
  }
  if (query.checks.length > 0 && !query.checks.includes(row.checks.state)) return false;
  return true;
}

const size = (r: PrRow) => r.additions + r.deletions;

export function sortRows(rows: PrRow[], sort: SortId): PrRow[] {
  const by: Record<SortId, (a: PrRow, b: PrRow) => number> = {
    updated: (a, b) => b.updated_at.localeCompare(a.updated_at),
    newest: (a, b) => b.created_at.localeCompare(a.created_at),
    oldest: (a, b) => a.created_at.localeCompare(b.created_at),
    largest: (a, b) => size(b) - size(a),
    smallest: (a, b) => size(a) - size(b),
  };
  return [...rows].sort((a, b) => by[sort](a, b) || b.number - a.number);
}

export type Group = { id: "review" | "mine" | "others"; title: string; rows: PrRow[] };

/** Pull requests that wait for you first, then yours, then the rest. Empty groups are left out. */
export function groupRows(rows: PrRow[]): Group[] {
  const groups: Group[] = [
    {
      id: "review",
      title: "Needs your review",
      rows: rows.filter((r) => r.review_requested && !r.by_me),
    },
    { id: "mine", title: "Yours", rows: rows.filter((r) => r.by_me) },
    { id: "others", title: "Others", rows: rows.filter((r) => !r.by_me && !r.review_requested) },
  ];
  return groups.filter((g) => g.rows.length > 0);
}
