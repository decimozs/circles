import { useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { projectKeys } from "@/features/projects/api";
import { prKeys } from "@/features/prs/api";

/** What each kind of answer kept from GitHub is read again under, when the background fetch of it
 * is done. The backend names the answer in the event. */
const READ_AGAIN: Record<string, readonly (readonly string[])[]> = {
  "pr-list": [prKeys.all],
  "pr-detail": [prKeys.detailAll],
  "github-repos": [projectKeys.githubRepos],
  "github-accounts": [["account", "github"]],
};

/** Reads again what is on screen when something kept was fetched again in the background. */
export function useCacheSync() {
  const queryClient = useQueryClient();
  useEffect(() => {
    let cancelled = false;
    let stop: (() => void) | undefined;
    listen<string>("cache:changed", (event) => {
      for (const queryKey of READ_AGAIN[event.payload] ?? []) {
        queryClient.invalidateQueries({ queryKey: [...queryKey] });
      }
    })
      .then((unlisten) => {
        if (cancelled) unlisten();
        else stop = unlisten;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      stop?.();
    };
  }, [queryClient]);
}
