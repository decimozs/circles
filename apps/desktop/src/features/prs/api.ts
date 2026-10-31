import type { BrowseState } from "@circles/bindings/BrowseState";
import type { PrFileRequest } from "@circles/bindings/PrFileRequest";
import { keepPreviousData, useQuery, type useQueryClient } from "@tanstack/react-query";
import { ipc } from "@/lib/ipc";

export const prKeys = {
  all: ["pr-browse"] as const,
  detailAll: ["pr-browse-detail"] as const,
  detail: (projectId: string, number: number) => ["pr-browse-detail", projectId, number] as const,
  files: (projectId: string, number: number) => ["pr-browse-files", projectId, number] as const,
  file: (r: PrFileRequest) =>
    ["pr-browse-file", r.project_id, r.base_sha, r.head_sha, r.path, r.status] as const,
  list: (projectId: string | null, which: BrowseState) => ["pr-browse", projectId, which] as const,
};

/** The pull requests of one project, or of all of them. What Circles kept comes back at once, and
 * it is asked from GitHub again in the background; `useCacheSync` reads again when that is done.
 * Only a repository seen for the first time waits for GitHub. While another project or state
 * loads, the list on screen stays. */
export function usePrList(projectId: string | null, which: BrowseState, enabled = true) {
  return useQuery({
    queryKey: prKeys.list(projectId, which),
    queryFn: () => ipc.prBrowseList(projectId, which),
    enabled,
    staleTime: 10_000,
    refetchOnWindowFocus: true,
    placeholderData: keepPreviousData,
  });
}

/** Asks GitHub now, and waits for the answer. */
export async function refreshPrList(
  queryClient: ReturnType<typeof useQueryClient>,
  projectId: string | null,
  which: BrowseState,
) {
  const fresh = await ipc.prBrowseList(projectId, which, true);
  queryClient.setQueryData(prKeys.list(projectId, which), fresh);
}

/** One pull request in full, kept the same way. */
export function usePrDetail(projectId: string, number: number) {
  return useQuery({
    queryKey: prKeys.detail(projectId, number),
    queryFn: () => ipc.prBrowseDetail(projectId, number),
    staleTime: 10_000,
  });
}

/** The files a pull request changes. */
export function usePrFiles(projectId: string, repo: string, number: number, headSha: string) {
  return useQuery({
    // A new tip of the branch is a new set of files.
    queryKey: [...prKeys.files(projectId, number), headSha],
    queryFn: () => ipc.prBrowseFiles(projectId, repo, number, headSha),
    staleTime: Number.POSITIVE_INFINITY,
  });
}

/** Both sides of one file. A commit never changes, so what was read is kept. */
export function usePrFile(request: PrFileRequest, enabled: boolean) {
  return useQuery({
    queryKey: prKeys.file(request),
    queryFn: () => ipc.prBrowseFile(request),
    enabled,
    staleTime: Number.POSITIVE_INFINITY,
  });
}
