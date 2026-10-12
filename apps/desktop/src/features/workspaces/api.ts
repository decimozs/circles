import type { CreateOptions } from "@circles/bindings/CreateOptions";
import type { StateChange } from "@circles/bindings/StateChange";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { projectKeys } from "@/features/projects/api";
import { ipc } from "@/lib/ipc";

export const workspaceKeys = {
  all: ["workspaces"] as const,
  one: (id: string) => ["workspaces", id] as const,
  events: (id: string) => ["workspaces", id, "events"] as const,
  sections: ["sections"] as const,
};

/** Every workspace, archived ones included. Callers filter. */
export function useWorkspaces() {
  return useQuery({ queryKey: workspaceKeys.all, queryFn: () => ipc.workspaceList() });
}

/**
 * Where each workspace's branch stands, by workspace id: for the sidebar's icons. One answer for
 * all of them, refreshed now and then and when the window is focused again.
 */
export function useBranchStates() {
  return useQuery({
    queryKey: [...workspaceKeys.all, "branch-states"] as const,
    queryFn: ipc.workspaceBranchStates,
    select: (list) => new Map(list.map((s) => [s.workspace_id, s])),
    refetchInterval: 60_000,
    refetchOnWindowFocus: true,
    staleTime: 20_000,
  });
}

export function useWorkspace(id: string | undefined) {
  return useQuery({
    queryKey: workspaceKeys.one(id ?? ""),
    queryFn: () => ipc.workspaceGet(id as string),
    enabled: !!id,
  });
}

export function useWorkspaceEvents(id: string | undefined) {
  return useQuery({
    queryKey: workspaceKeys.events(id ?? ""),
    queryFn: () => ipc.workspaceEvents(id as string),
    enabled: !!id,
  });
}

export function useSections() {
  return useQuery({ queryKey: workspaceKeys.sections, queryFn: ipc.sectionList });
}

/**
 * Keeps the workspace queries fresh when the backend announces a state change, including changes
 * made by the startup reconcile or a background creation. Mount once.
 */
export function useWorkspaceStateSync() {
  const queryClient = useQueryClient();
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    listen<StateChange>("workspace:state-changed", () => {
      queryClient.invalidateQueries({ queryKey: workspaceKeys.all });
      queryClient.invalidateQueries({ queryKey: projectKeys.all });
    })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {
        // Not running inside Tauri (for example a plain browser): nothing to listen to.
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [queryClient]);
}

/** Workspace changes also change project counts and the sidebar, so refresh both. */
function useWorkspaceMutation<TVars, TData>(fn: (vars: TVars) => Promise<TData>) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: fn,
    onSettled: () => {
      queryClient.invalidateQueries({ queryKey: workspaceKeys.all });
      queryClient.invalidateQueries({ queryKey: projectKeys.all });
    },
  });
}

export const useCreateWorkspace = () =>
  useWorkspaceMutation((vars: { projectId: string; options: CreateOptions }) =>
    ipc.workspaceCreate(vars.projectId, vars.options),
  );

/** Branches of a project for the Create dialog. Only runs when `enabled`. */
export function useBranches(projectId: string | undefined, enabled: boolean) {
  return useQuery({
    queryKey: ["branches", projectId],
    queryFn: () => ipc.projectBranches(projectId as string),
    enabled: enabled && !!projectId,
    staleTime: 10_000,
  });
}

/** Open pull requests of a project. Only runs when `enabled`. */
export function usePullRequests(projectId: string | undefined, enabled: boolean) {
  return useQuery({
    queryKey: ["pull-requests", projectId],
    queryFn: () => ipc.githubListPrs(projectId as string),
    enabled: enabled && !!projectId,
    staleTime: 30_000,
  });
}
export const useArchiveWorkspace = () =>
  useWorkspaceMutation((vars: { id: string; commitDirty: boolean }) =>
    ipc.workspaceArchive(vars.id, vars.commitDirty),
  );
export const useRestoreWorkspace = () => useWorkspaceMutation(ipc.workspaceRestore);
export const useRenameBranch = () =>
  useWorkspaceMutation((vars: { id: string; name: string }) =>
    ipc.workspaceRenameBranch(vars.id, vars.name),
  );
export const usePinWorkspace = () =>
  useWorkspaceMutation((vars: { id: string; pinned: boolean }) =>
    ipc.workspacePin(vars.id, vars.pinned),
  );
export const useDeleteWorkspace = () =>
  useWorkspaceMutation((vars: { id: string; deleteBranch: boolean }) =>
    ipc.workspaceDelete(vars.id, vars.deleteBranch),
  );
export const useSetWorkspaceSection = () =>
  useWorkspaceMutation((vars: { workspaceId: string; sectionId: string | null }) =>
    ipc.workspaceSetSection(vars.workspaceId, vars.sectionId),
  );

function useSectionMutation<TVars, TData>(fn: (vars: TVars) => Promise<TData>) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: fn,
    onSettled: () => {
      queryClient.invalidateQueries({ queryKey: workspaceKeys.sections });
      queryClient.invalidateQueries({ queryKey: workspaceKeys.all });
    },
  });
}

export const useCreateSection = () => useSectionMutation(ipc.sectionCreate);
export const useRenameSection = () =>
  useSectionMutation((vars: { id: string; name: string }) => ipc.sectionRename(vars.id, vars.name));
export const useDeleteSection = () => useSectionMutation(ipc.sectionDelete);
