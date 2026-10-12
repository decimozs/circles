import type { NewComment } from "@circles/bindings/NewComment";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { ipc } from "@/lib/ipc";

export type Scope = "all" | "uncommitted";

export const fileKeys = {
  list: (ws: string, path: string, showIgnored: boolean) =>
    ["fs", ws, "list", path, showIgnored] as const,
  file: (ws: string, path: string) => ["fs", ws, "file", path] as const,
  changes: (ws: string, scope: Scope) => ["changes", ws, scope] as const,
  pair: (ws: string, scope: Scope, path: string) => ["changes", ws, "pair", scope, path] as const,
  comments: (ws: string) => ["comments", ws] as const,
  editors: ["editors"] as const,
};

/** The entries of one folder of the workspace. */
export function useFsList(workspaceId: string, path: string, showIgnored: boolean, enabled = true) {
  return useQuery({
    queryKey: fileKeys.list(workspaceId, path, showIgnored),
    queryFn: () => ipc.fsList(workspaceId, path, showIgnored),
    enabled,
  });
}

export function useFileContent(workspaceId: string, path: string) {
  return useQuery({
    queryKey: fileKeys.file(workspaceId, path),
    queryFn: () => ipc.fsRead(workspaceId, path),
  });
}

export function useChanges(workspaceId: string | undefined, scope: Scope) {
  return useQuery({
    queryKey: fileKeys.changes(workspaceId ?? "", scope),
    queryFn: () => ipc.changesList(workspaceId as string, scope),
    enabled: !!workspaceId,
  });
}

export function useFilePair(
  workspaceId: string,
  scope: Scope,
  path: string,
  oldPath?: string | null,
) {
  return useQuery({
    queryKey: fileKeys.pair(workspaceId, scope, path),
    queryFn: () => ipc.diffFiles(workspaceId, scope, path, oldPath),
  });
}

export function useEditors() {
  return useQuery({ queryKey: fileKeys.editors, queryFn: ipc.editorList, staleTime: 60_000 });
}

/**
 * Watches the workspace's files while it is open, and refreshes the tree, the changes and the
 * open file when the backend reports a change. Mount where the workspace is shown.
 */
export function useFilesWatcher(workspaceId: string | undefined) {
  const queryClient = useQueryClient();
  useEffect(() => {
    if (!workspaceId) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    ipc.filesWatch(workspaceId).catch(() => {
      // The workspace may have no folder right now (archived): there is nothing to watch.
    });
    listen<{ workspace_id: string }>("workspace:files-changed", (event) => {
      if (event.payload.workspace_id !== workspaceId) return;
      queryClient.invalidateQueries({ queryKey: ["fs", workspaceId] });
      queryClient.invalidateQueries({ queryKey: ["changes", workspaceId] });
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
      ipc.filesUnwatch(workspaceId).catch(() => {});
    };
  }, [workspaceId, queryClient]);
}

export function useComments(workspaceId: string | undefined) {
  return useQuery({
    queryKey: fileKeys.comments(workspaceId ?? ""),
    queryFn: () => ipc.commentList(workspaceId as string),
    enabled: !!workspaceId,
  });
}

function useCommentMutation<TVars, TData>(
  workspaceId: string,
  fn: (vars: TVars) => Promise<TData>,
) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: fn,
    onSettled: () => queryClient.invalidateQueries({ queryKey: fileKeys.comments(workspaceId) }),
  });
}

export const useCreateComment = (workspaceId: string) =>
  useCommentMutation(workspaceId, (comment: NewComment) => ipc.commentCreate(workspaceId, comment));
export const useResolveComment = (workspaceId: string) =>
  useCommentMutation(workspaceId, (v: { id: string; resolved: boolean }) =>
    ipc.commentSetStatus(v.id, v.resolved),
  );
export const useDeleteComment = (workspaceId: string) =>
  useCommentMutation(workspaceId, (id: string) => ipc.commentDelete(id));
export const useSendComments = (workspaceId: string) =>
  useCommentMutation(workspaceId, (v: { sessionId: string; ids: string[] }) =>
    ipc.commentSend(workspaceId, v.sessionId, v.ids),
  );
