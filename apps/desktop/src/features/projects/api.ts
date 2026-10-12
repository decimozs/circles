import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ipc } from "@/lib/ipc";

export const projectKeys = {
  all: ["projects"] as const,
  githubRepos: ["github", "repos"] as const,
};

export function useProjects() {
  return useQuery({ queryKey: projectKeys.all, queryFn: ipc.projectList });
}

/** Repositories of the signed in GitHub user. Only runs when `enabled`. */
export function useGithubRepos(enabled: boolean) {
  return useQuery({
    queryKey: projectKeys.githubRepos,
    queryFn: ipc.githubListRepos,
    enabled,
    staleTime: 60_000,
  });
}

function useProjectMutation<TVars, TData>(fn: (vars: TVars) => Promise<TData>) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: fn,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: projectKeys.all }),
  });
}

export const useAddLocalProject = () => useProjectMutation(ipc.projectAddLocal);
export const useAddGithubProject = () => useProjectMutation(ipc.projectAddGithub);
export const useQuickStartProject = () => useProjectMutation(ipc.projectQuickStart);
export function useRemoveProject() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (vars: { id: string; workspaces?: "keep" | "delete" }) =>
      ipc.projectRemove(vars.id, vars.workspaces),
    onSuccess: () => {
      // Its workspaces go from the lists too.
      queryClient.invalidateQueries({ queryKey: projectKeys.all });
      queryClient.invalidateQueries({ queryKey: ["workspaces"] });
    },
  });
}
export const useSetBaseBranch = () =>
  useProjectMutation((vars: { id: string; branch: string }) =>
    ipc.projectSetBaseBranch(vars.id, vars.branch),
  );
