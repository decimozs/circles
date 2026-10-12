import { create } from "zustand";
import { persist } from "zustand/middleware";

/** Whether the first-run screen has been dealt with. */
export const useOnboarding = create<{
  done: boolean;
  /** The screen is wanted now: a launch with no projects, or "show again". Not remembered. */
  armed: boolean;
  finish: () => void;
  arm: () => void;
  again: () => void;
}>()(
  persist(
    (set) => ({
      done: false,
      armed: false,
      finish: () => set({ done: true, armed: false }),
      arm: () => set({ armed: true }),
      again: () => set({ done: false, armed: true }),
    }),
    { name: "circles-onboarding", partialize: (s) => ({ done: s.done }) },
  ),
);
