import type { BrowseState } from "@circles/bindings/BrowseState";
import type { Project } from "@circles/bindings/Project";
import { Button } from "@circles/ui/components/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@circles/ui/components/dropdown-menu";
import { Input } from "@circles/ui/components/input";
import { Popover, PopoverContent, PopoverTrigger } from "@circles/ui/components/popover";
import {
  ArrowDownUpIcon,
  FilterIcon,
  FolderGitIcon,
  RefreshCwIcon,
  SearchIcon,
} from "lucide-react";
import {
  type ChecksFilter,
  type DraftFilter,
  type Filters,
  filtersSet,
  type ReviewFilter,
  SORTS,
  type SortId,
} from "./query";

const STATES: { id: BrowseState; label: string }[] = [
  { id: "open", label: "Open" },
  { id: "merged", label: "Merged" },
  { id: "closed", label: "Closed" },
  { id: "all", label: "All states" },
];

function Field<T extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: T;
  options: { id: T; label: string }[];
  onChange: (value: T) => void;
}) {
  return (
    <label className="flex items-center justify-between gap-4 text-sm">
      {label}
      <select
        className="h-7 rounded-md border border-input bg-transparent px-1.5 text-sm"
        value={value}
        onChange={(e) => onChange(e.target.value as T)}
      >
        {options.map((o) => (
          <option key={o.id} value={o.id}>
            {o.label}
          </option>
        ))}
      </select>
    </label>
  );
}

/** The search box and the buttons beside it: sort, filters, which project, refresh. */
export function PrToolbar({
  query,
  onQuery,
  sort,
  onSort,
  filters,
  onFilters,
  which,
  onWhich,
  projects,
  scope,
  onScope,
  fetching,
  onRefresh,
}: {
  query: string;
  onQuery: (query: string) => void;
  sort: SortId;
  onSort: (sort: SortId) => void;
  filters: Filters;
  onFilters: (filters: Filters) => void;
  which: BrowseState;
  onWhich: (which: BrowseState) => void;
  projects: Project[];
  scope: string | null;
  onScope: (scope: string | null) => void;
  fetching: boolean;
  onRefresh: () => void;
}) {
  const set = filtersSet(filters) + Number(which !== "open");
  const scoped = projects.find((p) => p.id === scope);
  return (
    <div className="flex flex-col gap-2">
      <div className="relative">
        <SearchIcon className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground" />
        <Input
          aria-label="Search pull requests"
          placeholder="Search pull requests, or label:bug"
          value={query}
          onChange={(e) => onQuery(e.target.value)}
          className="h-9 pl-9"
        />
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <DropdownMenu>
          <DropdownMenuTrigger render={<Button variant="outline" size="sm" />}>
            <ArrowDownUpIcon />
            Sort
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="w-48">
            <DropdownMenuRadioGroup value={sort} onValueChange={(v) => onSort(v as SortId)}>
              {SORTS.map((s) => (
                <DropdownMenuRadioItem key={s.id} value={s.id}>
                  {s.label}
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          </DropdownMenuContent>
        </DropdownMenu>

        <Popover>
          <PopoverTrigger render={<Button variant="outline" size="sm" />}>
            <FilterIcon />
            Filters
            {set > 0 ? (
              <span className="rounded-full bg-primary px-1.5 text-2xs text-primary-foreground">
                {set}
              </span>
            ) : null}
          </PopoverTrigger>
          <PopoverContent align="start" className="flex w-72 flex-col gap-3">
            <Field label="State" value={which} options={STATES} onChange={(v) => onWhich(v)} />
            <Field<DraftFilter>
              label="Drafts"
              value={filters.draft}
              options={[
                { id: "any", label: "Show" },
                { id: "hide", label: "Hide" },
                { id: "only", label: "Only drafts" },
              ]}
              onChange={(draft) => onFilters({ ...filters, draft })}
            />
            <Field<ChecksFilter>
              label="Checks"
              value={filters.checks}
              options={[
                { id: "any", label: "Any" },
                { id: "passing", label: "Passing" },
                { id: "failing", label: "Failing" },
                { id: "pending", label: "Pending" },
              ]}
              onChange={(checks) => onFilters({ ...filters, checks })}
            />
            <Field<ReviewFilter>
              label="Review"
              value={filters.review}
              options={[
                { id: "any", label: "Any" },
                { id: "approved", label: "Approved" },
                { id: "changes_requested", label: "Changes requested" },
                { id: "review_required", label: "Needs review" },
              ]}
              onChange={(review) => onFilters({ ...filters, review })}
            />
          </PopoverContent>
        </Popover>

        <DropdownMenu>
          <DropdownMenuTrigger render={<Button variant="outline" size="sm" />}>
            <FolderGitIcon />
            {scoped ? scoped.name : "All projects"}
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="w-56">
            <DropdownMenuGroup>
              <DropdownMenuLabel>Show pull requests of</DropdownMenuLabel>
              <DropdownMenuSeparator />
              <DropdownMenuRadioGroup
                value={scope ?? ""}
                onValueChange={(v) => onScope(v === "" ? null : v)}
              >
                <DropdownMenuRadioItem value="">All projects</DropdownMenuRadioItem>
                {projects.map((p) => (
                  <DropdownMenuRadioItem key={p.id} value={p.id}>
                    {p.name}
                  </DropdownMenuRadioItem>
                ))}
              </DropdownMenuRadioGroup>
            </DropdownMenuGroup>
          </DropdownMenuContent>
        </DropdownMenu>

        <Button variant="outline" size="icon-sm" aria-label="Refresh" onClick={onRefresh}>
          <RefreshCwIcon className={fetching ? "animate-spin" : undefined} />
        </Button>
      </div>
    </div>
  );
}
