import { useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { ipc } from "@/lib/ipc";

export const graphKeys = {
  one: (workspaceId: string) => ["git-graph", workspaceId] as const,
};

/** How often the graph looks again while it is on screen, in case an event is missed. */
export const SAFETY_REFRESH_MS = 20_000;

/**
 * The graph of a workspace's branch. It follows the workspace: file, commit, push and fetch
 * events from the backend refresh it, and so does coming back to the window.
 */
export function useGitGraph(workspaceId: string | undefined, enabled = true) {
  const queryClient = useQueryClient();
  useEffect(() => {
    if (!workspaceId || !enabled) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    listen<{ workspace_id: string }>("workspace:files-changed", (event) => {
      if (event.payload.workspace_id === workspaceId) {
        queryClient.invalidateQueries({ queryKey: graphKeys.one(workspaceId) });
      }
    })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {
        // Not running inside Tauri: nothing to listen to.
      });
    const onFocus = () => queryClient.invalidateQueries({ queryKey: graphKeys.one(workspaceId) });
    window.addEventListener("focus", onFocus);
    return () => {
      cancelled = true;
      unlisten?.();
      window.removeEventListener("focus", onFocus);
    };
  }, [workspaceId, enabled, queryClient]);

  return useQuery({
    queryKey: graphKeys.one(workspaceId ?? ""),
    queryFn: () => ipc.gitGraph(workspaceId as string),
    enabled: Boolean(workspaceId) && enabled,
    refetchInterval: enabled ? SAFETY_REFRESH_MS : false,
    // Keep showing the old graph while a new one loads, so it does not flash.
    placeholderData: (previous) => previous,
  });
}
