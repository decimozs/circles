import { create } from "zustand";
import { persist } from "zustand/middleware";

export const SIDEBAR = { default: 320, min: 220, max: 480 } as const;
export const RIGHT_PANEL = { default: 490, min: 280, max: 800 } as const;
/** The pane beside the list of pull requests, which opens one of them. */
export const PR_PANE = { default: 640, min: 360, max: 1100 } as const;
/** `share` is the part of the window height the terminal opens at, until the user drags it. */
export const DOCK = { share: 0.3, min: 120, max: 800, collapsed: 0 } as const;

type UiState = {
  leftOpen: boolean;
  rightOpen: boolean;
  zen: boolean;
  leftWidth: number;
  rightWidth: number;
  /** Width of the pull request pane the user dragged it to. */
  prPaneWidth: number;
  /** The terminal dock under the right panel. */
  dockOpen: boolean;
  /** The height the user dragged the terminal to. Unset means the default share of the window. */
  dockHeight: number | null;
  /** The terminal takes the whole center column, and the chat is out of the way. */
  bigTerminal: boolean;
  /** Selected terminal tab for each workspace. */
  dockTab: Record<string, string>;
  shortcutsOpen: boolean;
  /** The popover of a Composer run, opened from the top bar. */
  runOpen: boolean;
  /** The Usage dialog. */
  usageOpen: boolean;
  addProjectOpen: boolean;
  /** Tab the Add project dialog opens on. */
  addProjectTab: "local" | "github" | "quick";
  createOpen: boolean;
  /** Project the Create dialog preselects, when opened from a project's + button. */
  createProjectId: string | null;
  paletteOpen: boolean;
  createSectionOpen: boolean;
  /** The dialog that creates a pull request. */
  prOpen: boolean;
  /** The dialog that commits uncommitted changes before a pull request. */
  commitOpen: boolean;
  setLeftOpen: (open: boolean) => void;
  setRightOpen: (open: boolean) => void;
  toggleLeft: () => void;
  toggleRight: () => void;
  toggleZen: () => void;
  setLeftWidth: (width: number) => void;
  setRightWidth: (width: number) => void;
  setDockOpen: (open: boolean) => void;
  toggleDock: () => void;
  setDockHeight: (height: number) => void;
  setBigTerminal: (on: boolean) => void;
  toggleBigTerminal: () => void;
  selectDockTab: (workspaceId: string, terminalId: string) => void;
  setShortcutsOpen: (open: boolean) => void;
  setUsageOpen: (open: boolean) => void;
  setPrPaneWidth: (width: number) => void;
  setRunOpen: (open: boolean) => void;
  setAddProjectOpen: (open: boolean, tab?: "local" | "github" | "quick") => void;
  openCreate: (projectId?: string | null) => void;
  setCreateOpen: (open: boolean) => void;
  setPaletteOpen: (open: boolean) => void;
  setCreateSectionOpen: (open: boolean) => void;
  setPrOpen: (open: boolean) => void;
  setCommitOpen: (open: boolean) => void;
};

export const useUi = create<UiState>()(
  persist(
    (set) => ({
      leftOpen: true,
      rightOpen: true,
      zen: false,
      leftWidth: SIDEBAR.default,
      rightWidth: RIGHT_PANEL.default,
      prPaneWidth: PR_PANE.default,
      dockOpen: false,
      dockHeight: null,
      bigTerminal: false,
      dockTab: {},
      shortcutsOpen: false,
      runOpen: false,
      usageOpen: false,
      addProjectOpen: false,
      addProjectTab: "local",
      createOpen: false,
      createProjectId: null,
      paletteOpen: false,
      createSectionOpen: false,
      prOpen: false,
      commitOpen: false,
      setLeftOpen: (leftOpen) => set({ leftOpen }),
      setRightOpen: (rightOpen) => set({ rightOpen }),
      toggleLeft: () => set((s) => ({ leftOpen: !s.leftOpen })),
      toggleRight: () => set((s) => ({ rightOpen: !s.rightOpen })),
      toggleZen: () => set((s) => ({ zen: !s.zen })),
      setLeftWidth: (leftWidth) => set({ leftWidth }),
      setRightWidth: (rightWidth) => set({ rightWidth }),
      setPrPaneWidth: (prPaneWidth) => set({ prPaneWidth }),
      // A terminal that is hidden is not maximised, and a maximised one is open.
      setDockOpen: (dockOpen) =>
        set((s) => ({ dockOpen, bigTerminal: dockOpen ? s.bigTerminal : false })),
      toggleDock: () =>
        set((s) => ({ dockOpen: !s.dockOpen, bigTerminal: s.dockOpen ? false : s.bigTerminal })),
      setDockHeight: (dockHeight) => set({ dockHeight }),
      setBigTerminal: (bigTerminal) =>
        set((s) => ({ bigTerminal, dockOpen: bigTerminal ? true : s.dockOpen })),
      toggleBigTerminal: () =>
        set((s) => ({ bigTerminal: !s.bigTerminal, dockOpen: s.bigTerminal ? s.dockOpen : true })),
      selectDockTab: (workspaceId, terminalId) =>
        set((s) => ({ dockTab: { ...s.dockTab, [workspaceId]: terminalId } })),
      setShortcutsOpen: (shortcutsOpen) => set({ shortcutsOpen }),
      setUsageOpen: (usageOpen) => set({ usageOpen }),
      setRunOpen: (runOpen) => set({ runOpen }),
      setAddProjectOpen: (addProjectOpen, tab) =>
        set((s) => ({ addProjectOpen, addProjectTab: tab ?? s.addProjectTab })),
      openCreate: (projectId = null) => set({ createOpen: true, createProjectId: projectId }),
      setCreateOpen: (createOpen) => set({ createOpen }),
      setPaletteOpen: (paletteOpen) => set({ paletteOpen }),
      setCreateSectionOpen: (createSectionOpen) => set({ createSectionOpen }),
      setPrOpen: (prOpen) => set({ prOpen }),
      setCommitOpen: (commitOpen) => set({ commitOpen }),
    }),
    {
      name: "circles-ui",
      version: 11,
      migrate: (persisted, version) => {
        const state = persisted as Partial<UiState>;
        // Earlier versions saved the sizes the panels took by themselves, such as the smallest
        // when the window was narrow, as if they were choices. Start again from the defaults.
        if (version < 5) {
          state.leftWidth = SIDEBAR.default;
          state.rightWidth = RIGHT_PANEL.default;
        }
        // From version 5 on, only sizes the user set are saved. A width equal to the default of
        // that version was never chosen, so it follows the new default.
        if (version === 5) {
          if (state.leftWidth === 340) state.leftWidth = SIDEBAR.default;
          if (state.rightWidth === 480) state.rightWidth = RIGHT_PANEL.default;
        }
        if (version === 6 && state.rightWidth === 430) state.rightWidth = RIGHT_PANEL.default;
        if (version === 7 && state.rightWidth === 460) state.rightWidth = RIGHT_PANEL.default;
        // The terminal moved from the right pane to the bottom of the chat, and starts closed.
        if (version < 9) state.dockOpen = false;
        // A height equal to an old default was never chosen. The terminal now opens at a share of
        // the window instead.
        if (version < 11 && (state.dockHeight === 280 || state.dockHeight === 360)) {
          state.dockHeight = null;
        }
        return state as UiState;
      },
      partialize: (s) => ({
        leftOpen: s.leftOpen,
        rightOpen: s.rightOpen,
        runOpen: s.runOpen,
        zen: s.zen,
        leftWidth: s.leftWidth,
        rightWidth: s.rightWidth,
        prPaneWidth: s.prPaneWidth,
        dockOpen: s.dockOpen,
        dockHeight: s.dockHeight,
      }),
    },
  ),
);
