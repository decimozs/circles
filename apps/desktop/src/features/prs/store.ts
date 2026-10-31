import type { BrowseState } from "@circles/bindings/BrowseState";
import type { PrRow } from "@circles/bindings/PrRow";
import { create } from "zustand";
import { persist } from "zustand/middleware";
import { type Filters, NO_FILTERS, type SortId } from "./query";

/** A pull request opened beside the list. */
export type PrTab = {
  key: string;
  projectId: string;
  repo: string;
  number: number;
  title: string;
  url: string;
};

export const tabKey = (projectId: string, number: number) => `${projectId}#${number}`;

type PrsState = {
  /** Pull requests opened beside the list. They are not kept between runs: what they show changes. */
  tabs: PrTab[];
  /** The open tab that shows, or none when only the list does. */
  selected: string | null;
  openTab: (row: PrRow) => void;
  closeTab: (key: string) => void;
  selectTab: (key: string | null) => void;
  /** Files marked as viewed, by pull request and tip of its branch, so a new push starts again. */
  viewed: Record<string, string[]>;
  setViewed: (pr: string, path: string, viewed: boolean) => void;
  /** The project whose pull requests show, `null` for all of them, and unset until the person
   * chooses, when the project they worked in last is used. */
  scope: string | null | undefined;
  which: BrowseState;
  sort: SortId;
  filters: Filters;
  query: string;
  setScope: (scope: string | null) => void;
  setWhich: (which: BrowseState) => void;
  setSort: (sort: SortId) => void;
  setFilters: (filters: Filters) => void;
  setQuery: (query: string) => void;
};

export const usePrs = create<PrsState>()(
  persist(
    (set) => ({
      tabs: [],
      selected: null,
      openTab: (row) =>
        set((s) => {
          const key = tabKey(row.project_id, row.number);
          const tab: PrTab = {
            key,
            projectId: row.project_id,
            repo: row.repo,
            number: row.number,
            title: row.title,
            url: row.url,
          };
          return {
            tabs: s.tabs.some((t) => t.key === key) ? s.tabs : [...s.tabs, tab],
            selected: key,
          };
        }),
      closeTab: (key) =>
        set((s) => {
          const index = s.tabs.findIndex((t) => t.key === key);
          const tabs = s.tabs.filter((t) => t.key !== key);
          // Closing the one that shows moves to its neighbour, or back to the list alone.
          const next = tabs[Math.min(index, tabs.length - 1)]?.key ?? null;
          return { tabs, selected: s.selected === key ? next : s.selected };
        }),
      selectTab: (selected) => set({ selected }),
      viewed: {},
      setViewed: (pr, path, viewed) =>
        set((s) => {
          const now = (s.viewed[pr] ?? []).filter((p) => p !== path);
          return { viewed: { ...s.viewed, [pr]: viewed ? [...now, path] : now } };
        }),
      scope: undefined,
      which: "open",
      sort: "updated",
      filters: NO_FILTERS,
      query: "",
      setScope: (scope) => set({ scope }),
      setWhich: (which) => set({ which }),
      setSort: (sort) => set({ sort }),
      setFilters: (filters) => set({ filters }),
      setQuery: (query) => set({ query }),
    }),
    {
      name: "circles-prs",
      version: 1,
      // What was typed in the box is not worth bringing back.
      partialize: ({ scope, which, sort, filters, viewed }) => ({
        scope,
        which,
        sort,
        filters,
        viewed,
      }),
    },
  ),
);
