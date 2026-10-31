import type { BrowseState } from "@circles/bindings/BrowseState";
import { useQuery } from "@tanstack/react-query";
import { ipc } from "@/lib/ipc";

export const prKeys = {
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
