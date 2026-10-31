import type { BrowseState } from "@circles/bindings/BrowseState";
import { useQuery } from "@tanstack/react-query";
import { ipc } from "@/lib/ipc";

export const prKeys = {
  detail: (projectId: string, number: number) => ["pr-browse-detail", projectId, number] as const,
  list: (projectId: string | null, which: BrowseState) => ["pr-browse", projectId, which] as const,
};

/** The pull requests of one project, or of all of them. Asking GitHub takes a moment, so what was
 * found stays for half a minute. */
export function usePrList(projectId: string | null, which: BrowseState, enabled = true) {
  return useQuery({
    queryKey: prKeys.list(projectId, which),
    queryFn: () => ipc.prBrowseList(projectId, which),
    enabled,
    staleTime: 30_000,
    refetchOnWindowFocus: true,
  });
}

/** One pull request in full. */
export function usePrDetail(projectId: string, number: number) {
  return useQuery({
    queryKey: prKeys.detail(projectId, number),
    queryFn: () => ipc.prBrowseDetail(projectId, number),
    staleTime: 30_000,
  });
}
