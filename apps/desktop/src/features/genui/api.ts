import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { ipc } from "@/lib/ipc";

export const genuiKeys = {
  cards: (sessionId: string) => ["genui-cards", sessionId] as const,
  pending: ["genui-pending"] as const,
};

/** The cards of a chat, oldest first. */
export function useCards(sessionId: string | undefined) {
  return useQuery({
    queryKey: genuiKeys.cards(sessionId ?? ""),
    queryFn: () => ipc.genuiCards(sessionId as string),
    enabled: !!sessionId,
  });
}

/** Every card that waits for a person. */
export function usePendingCards() {
  return useQuery({ queryKey: genuiKeys.pending, queryFn: ipc.genuiPending });
}

/** Keeps the cards fresh when one is shown, answered or closed. Mount once. */
export function useGenuiSync() {
  const queryClient = useQueryClient();
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    listen("genui:changed", () => {
      queryClient.invalidateQueries({ queryKey: ["genui-cards"] });
      queryClient.invalidateQueries({ queryKey: genuiKeys.pending });
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

function useRefreshing<TVars>(fn: (vars: TVars) => Promise<unknown>) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: fn,
    onSettled: () => {
      queryClient.invalidateQueries({ queryKey: ["genui-cards"] });
      queryClient.invalidateQueries({ queryKey: genuiKeys.pending });
    },
  });
}

export const useAnswerCard = () =>
  useRefreshing((v: { uid: string; values: Record<string, unknown> }) =>
    ipc.genuiAnswer(v.uid, v.values),
  );

export const useDismissCard = () => useRefreshing((uid: string) => ipc.genuiDismiss(uid));
