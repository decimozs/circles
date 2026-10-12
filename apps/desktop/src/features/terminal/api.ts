import type { TerminalChange } from "@circles/bindings/TerminalChange";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { ipc } from "@/lib/ipc";

export const terminalKeys = {
  all: ["terminals"] as const,
  list: (workspaceId: string) => ["terminals", workspaceId] as const,
  scripts: (workspaceId: string) => ["scripts", workspaceId] as const,
};

export function useTerminals(workspaceId: string | undefined) {
  return useQuery({
    queryKey: terminalKeys.list(workspaceId ?? ""),
    queryFn: () => ipc.terminalList(workspaceId as string),
    enabled: !!workspaceId,
  });
}

/** The setup, run and archive scripts configured for the workspace's project. */
export function useScripts(workspaceId: string | undefined) {
  return useQuery({
    queryKey: terminalKeys.scripts(workspaceId ?? ""),
    queryFn: () => ipc.scriptsGet(workspaceId as string),
    enabled: !!workspaceId,
  });
}

/** Refreshes the tab lists when a process starts or ends. Mount once. */
export function useTerminalSync() {
  const queryClient = useQueryClient();
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    listen<TerminalChange>("terminal:changed", () => {
      queryClient.invalidateQueries({ queryKey: terminalKeys.all });
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
  }, [queryClient]);
}

function useTerminalMutation<TVars, TData>(fn: (vars: TVars) => Promise<TData>) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: fn,
    onSettled: () => queryClient.invalidateQueries({ queryKey: terminalKeys.all }),
  });
}

export const useCreateTerminal = () =>
  useTerminalMutation((workspaceId: string) => ipc.terminalCreate(workspaceId));
export const useDeleteTerminal = () => useTerminalMutation(ipc.terminalDelete);
export const useStopTerminal = () => useTerminalMutation(ipc.terminalStop);
export const useRunScript = () =>
  useTerminalMutation((vars: { workspaceId: string; name?: string }) =>
    ipc.scriptRun(vars.workspaceId, vars.name),
  );
export const useStopScript = () => useTerminalMutation(ipc.scriptStop);
export const useRunSetup = () => useTerminalMutation(ipc.scriptRunSetup);
