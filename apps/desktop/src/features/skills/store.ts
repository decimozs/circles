import type { SkillRow } from "@circles/bindings/SkillRow";
import { create } from "zustand";
import { persist } from "zustand/middleware";
import { type Filters, NO_FILTERS, type SortId } from "./query";

/** A skill opened beside the list. */
export type SkillTab = { key: string; name: string; path: string };

type SkillsState = {
  /** Skills opened beside the list. They are not kept between runs. */
  tabs: SkillTab[];
  /** The open tab that shows, or none when only the list does. */
  selected: string | null;
  openTab: (row: Pick<SkillRow, "id" | "name" | "path">) => void;
  closeTab: (key: string) => void;
  selectTab: (key: string | null) => void;
  /** The project whose skills show, `null` for all of them. */
  scope: string | null;
  sort: SortId;
  filters: Filters;
  query: string;
  setScope: (scope: string | null) => void;
  setSort: (sort: SortId) => void;
  setFilters: (filters: Filters) => void;
  setQuery: (query: string) => void;
};

export const useSkillsUi = create<SkillsState>()(
  persist(
    (set) => ({
      tabs: [],
      selected: null,
      openTab: (row) =>
        set((s) => ({
          tabs: s.tabs.some((t) => t.key === row.id)
            ? s.tabs
            : [...s.tabs, { key: row.id, name: row.name, path: row.path }],
          selected: row.id,
        })),
      closeTab: (key) =>
        set((s) => {
          const index = s.tabs.findIndex((t) => t.key === key);
          const tabs = s.tabs.filter((t) => t.key !== key);
          // Closing the one that shows moves to its neighbour, or back to the list alone.
          const next = tabs[Math.min(index, tabs.length - 1)]?.key ?? null;
          return { tabs, selected: s.selected === key ? next : s.selected };
        }),
      selectTab: (selected) => set({ selected }),
      scope: null,
      sort: "name",
      filters: NO_FILTERS,
      query: "",
      setScope: (scope) => set({ scope }),
      setSort: (sort) => set({ sort }),
      setFilters: (filters) => set({ filters }),
      setQuery: (query) => set({ query }),
    }),
    {
      name: "circles-skills",
      version: 1,
      partialize: ({ scope, sort, filters }) => ({ scope, sort, filters }),
    },
  ),
);
