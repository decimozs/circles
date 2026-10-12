import { create } from "zustand";
import { type Shortcut, shortcutKey } from "@/lib/shortcuts";

export const ACTION_GROUPS = [
  "Navigation",
  "Workspaces",
  "Chat",
  "Review",
  "Git",
  "Terminal",
  "Composers",
  "App",
] as const;

export type ActionGroup = (typeof ACTION_GROUPS)[number];

export type Action = {
  id: string;
  title: string;
  group: ActionGroup;
  /** The first shortcut is the one displayed. */
  shortcuts?: Shortcut[];
  keywords?: string[];
  run: () => void;
};

type Registry = {
  /** Actions per registration source, so a feature can replace its own set. */
  sources: Record<string, Action[]>;
  register: (source: string, actions: Action[]) => () => void;
};

export const useActionRegistry = create<Registry>((set) => ({
  sources: {},
  register: (source, actions) => {
    set((state) => ({ sources: { ...state.sources, [source]: actions } }));
    return () =>
      set((state) => {
        const { [source]: _removed, ...rest } = state.sources;
        return { sources: rest };
      });
  },
}));

export function flattenActions(sources: Record<string, Action[]>): Action[] {
  return Object.values(sources).flat();
}

/** Dev-time check: warns when two actions claim the same shortcut. */
export function findShortcutConflicts(actions: Action[]): string[] {
  const seen = new Map<string, string>();
  const conflicts: string[] = [];
  for (const action of actions) {
    for (const shortcut of action.shortcuts ?? []) {
      const key = shortcutKey(shortcut);
      const owner = seen.get(key);
      if (owner && owner !== action.id) conflicts.push(`${key}: ${owner} and ${action.id}`);
      else seen.set(key, action.id);
    }
  }
  return conflicts;
}
