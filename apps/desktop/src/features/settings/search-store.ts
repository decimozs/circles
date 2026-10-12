import { create } from "zustand";

/** The settings search lives in the left pane, its results in the page, so both share this. */
export const useSettingsSearch = create<{ query: string; setQuery: (q: string) => void }>(
  (set) => ({ query: "", setQuery: (query) => set({ query }) }),
);
