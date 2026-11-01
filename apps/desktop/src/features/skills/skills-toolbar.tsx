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
import { type Filters, filtersSet, type ModelFilter, SORTS, type SortId } from "./query";

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
export function SkillsToolbar({
  query,
  onQuery,
  sort,
  onSort,
  filters,
  onFilters,
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
  projects: Project[];
  scope: string | null;
  onScope: (scope: string | null) => void;
  fetching: boolean;
  onRefresh: () => void;
}) {
  const set = filtersSet(filters);
  const scoped = projects.find((p) => p.id === scope);
  return (
    <div className="flex flex-col gap-2">
      <div className="relative">
        <SearchIcon className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground" />
        <Input
          aria-label="Search skills"
          placeholder="Search skills, or group:project"
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
            <Field<Filters["group"]>
              label="Group"
              value={filters.group}
              options={[
                { id: "all", label: "All" },
                { id: "yours", label: "Yours" },
                { id: "shared", label: "Shared" },
                { id: "plugin", label: "Plugins" },
                { id: "project", label: "Projects" },
              ]}
              onChange={(group) => onFilters({ ...filters, group })}
            />
            <Field<ModelFilter>
              label="The model may use it"
              value={filters.model}
              options={[
                { id: "any", label: "Any" },
                { id: "yes", label: "Yes" },
                { id: "no", label: "Manual only" },
              ]}
              onChange={(model) => onFilters({ ...filters, model })}
            />
            <label className="flex items-center justify-between gap-4 text-sm">
              Has scripts or other files
              <input
                type="checkbox"
                checked={filters.files}
                onChange={(e) => onFilters({ ...filters, files: e.target.checked })}
              />
            </label>
          </PopoverContent>
        </Popover>

        <DropdownMenu>
          <DropdownMenuTrigger render={<Button variant="outline" size="sm" />}>
            <FolderGitIcon />
            {scoped ? scoped.name : "All projects"}
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="w-56">
            <DropdownMenuGroup>
              <DropdownMenuLabel>Show the skills of</DropdownMenuLabel>
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
