import { create } from "zustand";
import { persist } from "zustand/middleware";
import { DEFAULT_EFFORT, DEFAULT_MODEL, type EffortId, type ModelId } from "@/lib/models";

/** What the Create dialog remembers between uses. */
type CreatePrefs = {
  projectId: string | null;
  model: ModelId;
  effort: EffortId;
  /** The defaults from the settings that were last applied, so a later pick in the dialog stays. */
  appliedDefault: { model: string | null; effort: string | null };
  set: (
    patch: Partial<Pick<CreatePrefs, "projectId" | "model" | "effort" | "appliedDefault">>,
  ) => void;
};

export const useCreatePrefs = create<CreatePrefs>()(
  persist(
    (set) => ({
      projectId: null,
      model: DEFAULT_MODEL,
      effort: DEFAULT_EFFORT,
      appliedDefault: { model: null, effort: null },
      set: (patch) => set(patch),
    }),
    {
      name: "circles-create-prefs",
      partialize: (s) => ({
        projectId: s.projectId,
        model: s.model,
        effort: s.effort,
        appliedDefault: s.appliedDefault,
      }),
    },
  ),
);
