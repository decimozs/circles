import type { BrowseState } from "@circles/bindings/BrowseState";
import { create } from "zustand";
import { persist } from "zustand/middleware";
import { type Filters, NO_FILTERS, type SortId } from "./query";

type PrsState = {
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
      partialize: ({ scope, which, sort, filters }) => ({ scope, which, sort, filters }),
    },
  ),
);
