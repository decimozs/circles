import type { ComposerInput } from "@circles/bindings/ComposerInput";
import { create } from "zustand";

/** The create dialog. `seed` pre-fills a new Composer (from a template). */
type DialogState = {
  dialog: { mode: "create"; seed?: Partial<ComposerInput> } | null;
  openCreate: (seed?: Partial<ComposerInput>) => void;
  close: () => void;
};

export const useComposerDialog = create<DialogState>((set) => ({
  dialog: null,
  openCreate: (seed) => set({ dialog: { mode: "create", seed } }),
  close: () => set({ dialog: null }),
}));
