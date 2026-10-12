import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { Scope } from "@/features/files/api";

export type RightTab = "files" | "changes" | "checks" | "review";

/** What the center column shows instead of the chat. */
export type CenterView =
  | { kind: "diff"; path: string; oldPath: string | null }
  | { kind: "file"; path: string };

type ReviewState = {
  rightTab: RightTab;
  /** Which changes the Changes tab and the diff viewer compare. */
  scope: Scope;
  groupByFolder: boolean;
  showIgnored: boolean;
  /** The git graph at the bottom of the right pane is open. */
  graphOpen: boolean;
  /** Height of the graph in pixels, or `null` for the usual share of the pane. */
  graphHeight: number | null;
  /** Side by side (`true`) or one column (`false`) in the diff viewer. */
  split: boolean;
  views: Record<string, CenterView | undefined>;
  setRightTab: (tab: RightTab) => void;
  setScope: (scope: Scope) => void;
  setGroupByFolder: (on: boolean) => void;
  setShowIgnored: (on: boolean) => void;
  setSplit: (split: boolean) => void;
  toggleGraph: () => void;
  setGraphHeight: (height: number | null) => void;
  openDiff: (workspaceId: string, path: string, oldPath?: string | null) => void;
  openFile: (workspaceId: string, path: string) => void;
  closeView: (workspaceId: string) => void;
};

export const useReview = create<ReviewState>()(
  persist(
    (set) => ({
      rightTab: "files",
      scope: "uncommitted",
      groupByFolder: false,
      showIgnored: false,
      graphOpen: true,
      graphHeight: null,
      split: true,
      views: {},
      setRightTab: (rightTab) => set({ rightTab }),
      setScope: (scope) => set({ scope }),
      setGroupByFolder: (groupByFolder) => set({ groupByFolder }),
      setShowIgnored: (showIgnored) => set({ showIgnored }),
      setSplit: (split) => set({ split }),
      toggleGraph: () => set((s) => ({ graphOpen: !s.graphOpen })),
      setGraphHeight: (graphHeight) => set({ graphHeight }),
      openDiff: (workspaceId, path, oldPath = null) =>
        set((s) => ({
          views: { ...s.views, [workspaceId]: { kind: "diff", path, oldPath } },
        })),
      openFile: (workspaceId, path) =>
        set((s) => ({ views: { ...s.views, [workspaceId]: { kind: "file", path } } })),
      closeView: (workspaceId) => set((s) => ({ views: { ...s.views, [workspaceId]: undefined } })),
    }),
    {
      name: "circles-review",
      partialize: (s) => ({
        rightTab: s.rightTab,
        scope: s.scope,
        groupByFolder: s.groupByFolder,
        showIgnored: s.showIgnored,
        split: s.split,
        graphOpen: s.graphOpen,
        graphHeight: s.graphHeight,
      }),
    },
  ),
);
