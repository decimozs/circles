import type { SettingsView } from "@circles/bindings/SettingsView";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { ipc } from "@/lib/ipc";

export type WriteScope = "user" | "project" | "local";

export const settingsKeys = {
  all: ["settings-view"] as const,
  view: (projectId?: string) => ["settings-view", projectId ?? null] as const,
};

export function useSettingsView(projectId?: string) {
  return useQuery({
    queryKey: settingsKeys.view(projectId),
    queryFn: () => ipc.settingsView(projectId),
  });
}

/** Watches the settings files and refreshes every view when one changes outside the app. */
export function useSettingsSync(projectId?: string) {
  const queryClient = useQueryClient();
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    ipc.settingsWatch(projectId).catch(() => {});
    listen("settings:changed", () => {
      queryClient.invalidateQueries({ queryKey: settingsKeys.all });
    })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [queryClient, projectId]);
}

export type SetVars = { scope: WriteScope; path: string[]; value: unknown };

/** Writes (or, with `undefined`, removes) one setting and shows what the file now holds. */
export function useSetSetting(projectId?: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (vars: SetVars) => ipc.settingsSet(vars.scope, projectId, vars.path, vars.value),
    onSuccess: (view: SettingsView) => {
      queryClient.setQueryData(settingsKeys.view(projectId), view);
      queryClient.invalidateQueries({ queryKey: settingsKeys.all });
    },
  });
}

/** The value a layer holds at `path`, or `undefined` when it does not set it. */
export function valueAt(root: unknown, path: string[]): unknown {
  let cur: unknown = root;
  for (const key of path) {
    if (cur === null || typeof cur !== "object") return undefined;
    cur = (cur as Record<string, unknown>)[key];
  }
  return cur === null ? undefined : cur;
}

export function layerValue(view: SettingsView, scope: string, path: string[]): unknown {
  return valueAt(view.values.find((l) => l.scope === scope)?.values, path);
}
