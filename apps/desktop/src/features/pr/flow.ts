import type { QueryClient } from "@tanstack/react-query";
import { prKeys } from "@/features/pr/api";
import { ipc } from "@/lib/ipc";
import { useUi } from "@/stores/ui";

/**
 * Starts creating a pull request. A branch with nothing committed cannot have one, so when there
 * are only uncommitted changes the commit dialog comes first and opens the pull request dialog
 * when it is done.
 */
export async function startPullRequest(queryClient: QueryClient, workspaceId: string) {
  const ui = useUi.getState();
  try {
    const { git } = await queryClient.fetchQuery({
      queryKey: prKeys.readiness(workspaceId),
      queryFn: () => ipc.prReadiness(workspaceId),
      staleTime: 0,
    });
    if (git.ahead === 0 && git.dirty) ui.setCommitOpen(true);
    else ui.setPrOpen(true);
  } catch {
    // Without an answer from git, the pull request dialog explains what is wrong.
    ui.setPrOpen(true);
  }
}
