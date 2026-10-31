import type { BrowseState } from "@circles/bindings/BrowseState";
import type { PrFileRequest } from "@circles/bindings/PrFileRequest";
import { useQuery } from "@tanstack/react-query";
import { ipc } from "@/lib/ipc";

export const prKeys = {
  files: (projectId: string, number: number) => ["pr-browse-files", projectId, number] as const,
  file: (r: PrFileRequest) =>
    ["pr-browse-file", r.project_id, r.base_sha, r.head_sha, r.path, r.status] as const,
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

/** The files a pull request changes. */
export function usePrFiles(projectId: string, repo: string, number: number) {
  return useQuery({
    queryKey: prKeys.files(projectId, number),
    queryFn: () => ipc.prBrowseFiles(projectId, repo, number),
    staleTime: 30_000,
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
