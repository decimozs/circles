import type { Project } from "@circles/bindings/Project";
import { useNavigate } from "react-router";
import { toast } from "sonner";
import { useCreateWorkspace } from "@/features/workspaces/api";

/** Creates a workspace in `project` with default options, then opens it. */
export function useQuickCreateWorkspace() {
  const create = useCreateWorkspace();
  const navigate = useNavigate();

  return {
    isPending: create.isPending,
    create: (project: Pick<Project, "id" | "name">) =>
      create.mutate(
        {
          projectId: project.id,
          options: {
            source: { kind: "new_branch", base_branch: null },
            prompt: null,
            model: null,
            effort: null,
            attachments: [],
          },
        },
        {
          onSuccess: (workspace) => navigate(`/workspace/${workspace.id}`),
          onError: (error) =>
            toast.error(`Could not create a workspace in ${project.name}`, {
              description: error.message,
            }),
        },
      ),
  };
}
