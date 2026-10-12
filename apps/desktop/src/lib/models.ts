/** Models offered in the Create dialog. The `id` is what `claude --model` receives. */
export const MODELS = [
  { id: "fable", label: "Fable" },
  { id: "opus", label: "Opus" },
  { id: "sonnet", label: "Sonnet" },
  { id: "haiku", label: "Haiku" },
] as const;

/** Effort levels accepted by `claude --effort`. */
export const EFFORTS = [
  { id: "low", label: "Low" },
  { id: "medium", label: "Medium" },
  { id: "high", label: "High" },
  { id: "xhigh", label: "Extra high" },
  { id: "max", label: "Max" },
] as const;

export type ModelId = (typeof MODELS)[number]["id"];
export type EffortId = (typeof EFFORTS)[number]["id"];

export const DEFAULT_MODEL: ModelId = "opus";
export const DEFAULT_EFFORT: EffortId = "high";

export function modelLabel(id: string): string {
  return MODELS.find((m) => m.id === id)?.label ?? id;
}

export function effortLabel(id: string): string {
  return EFFORTS.find((e) => e.id === id)?.label ?? id;
}
