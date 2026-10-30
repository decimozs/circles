import { useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { ipc } from "@/lib/ipc";

export const prKeys = {
  readiness: (ws: string) => ["pr", ws, "readiness"] as const,
  checks: (ws: string) => ["pr", ws, "checks"] as const,
  threads: (ws: string) => ["pr", ws, "threads"] as const,
  deployments: (ws: string) => ["pr", ws, "deployments"] as const,
  all: (ws: string) => ["pr", ws] as const,
};

/** How often GitHub is asked again while a pull request is open, and while there is none. */
export const POLL_OPEN_MS = 60_000;
export const POLL_IDLE_MS = 120_000;

/**
 * Where the branch stands: git, the pull request, checks and reviews, and what to do next.
 * Refreshed on an interval and when the window gets focus again.
 */
export function useReadiness(workspaceId: string | undefined, enabled = true) {
  return useQuery({
    queryKey: prKeys.readiness(workspaceId ?? ""),
    queryFn: () => ipc.prReadiness(workspaceId as string),
    enabled: !!workspaceId && enabled,
    refetchInterval: (query) =>
      query.state.data?.pr?.state === "open" ? POLL_OPEN_MS : POLL_IDLE_MS,
    refetchOnWindowFocus: true,
    staleTime: 10_000,
  });
}

/** Refreshes what the Checks tab and the top bar show, after something changed. */
export function useRefreshPr(workspaceId: string) {
  const queryClient = useQueryClient();
  return () => queryClient.invalidateQueries({ queryKey: prKeys.all(workspaceId) });
}

/** Git state changes when files do, so the readiness follows the file watcher. */
export function useReadinessOnFileChanges(workspaceId: string | undefined) {
  const queryClient = useQueryClient();
  useEffect(() => {
    if (!workspaceId) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    listen<{ workspace_id: string }>("workspace:files-changed", (event) => {
      if (event.payload.workspace_id === workspaceId) {
        queryClient.invalidateQueries({ queryKey: prKeys.readiness(workspaceId) });
      }
    })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {
        // Not running inside Tauri: nothing to listen to.
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [workspaceId, queryClient]);
}

export function useChecks(workspaceId: string, enabled: boolean) {
  return useQuery({
    queryKey: prKeys.checks(workspaceId),
    queryFn: () => ipc.prChecks(workspaceId),
    enabled,
    refetchInterval: POLL_OPEN_MS,
    refetchOnWindowFocus: true,
  });
}

export function useThreads(workspaceId: string, enabled: boolean) {
  return useQuery({
    queryKey: prKeys.threads(workspaceId),
    queryFn: () => ipc.prThreads(workspaceId),
    enabled,
    refetchInterval: POLL_OPEN_MS,
    refetchOnWindowFocus: true,
  });
}

export function useDeployments(workspaceId: string, enabled: boolean) {
  return useQuery({
    queryKey: prKeys.deployments(workspaceId),
    queryFn: () => ipc.prDeployments(workspaceId),
    enabled,
    refetchOnWindowFocus: true,
  });
}
