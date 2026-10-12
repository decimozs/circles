import type { Project } from "@circles/bindings/Project";
import { Button } from "@circles/ui/components/button";
import { Input } from "@circles/ui/components/input";
import { Trash2Icon } from "lucide-react";
import { useState } from "react";
import { useProjects, useSetBaseBranch } from "@/features/projects/api";
import { ErrorMessage } from "@/features/projects/error-message";
import { ProjectAvatar } from "@/features/projects/project-avatar";
import { RemoveProjectDialog } from "@/features/projects/remove-project-dialog";
import { useUi } from "@/stores/ui";

function ProjectRow({ project }: { project: Project }) {
  const setBase = useSetBaseBranch();
  const [removing, setRemoving] = useState(false);
  const [branch, setBranch] = useState(project.default_branch);
  const changed = branch.trim() !== project.default_branch;

  return (
    <li className="flex flex-col gap-2 rounded-md border border-border p-3">
      <div className="flex items-center gap-2">
        <ProjectAvatar project={project} />
        <span className="font-medium">{project.name}</span>
        <span className="text-xs text-muted-foreground">
          {project.workspace_count} workspace{project.workspace_count === 1 ? "" : "s"}
        </span>
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={`Remove ${project.name}`}
          className="ml-auto text-muted-foreground"
          onClick={() => setRemoving(true)}
        >
          <Trash2Icon />
        </Button>
        <RemoveProjectDialog project={project} open={removing} onOpenChange={setRemoving} />
      </div>
      <p className="font-mono text-xs break-all text-muted-foreground">{project.path}</p>
      {project.remote_url ? (
        <p className="font-mono text-xs break-all text-muted-foreground">{project.remote_url}</p>
      ) : null}
      <form
        className="flex items-center gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          setBase.mutate(
            { id: project.id, branch },
            { onSuccess: (p) => setBranch(p.default_branch) },
          );
        }}
      >
        <label htmlFor={`base-${project.id}`} className="text-xs text-muted-foreground">
          Base branch
        </label>
        <Input
          id={`base-${project.id}`}
          value={branch}
          onChange={(e) => setBranch(e.target.value)}
          className="h-7 w-48 font-mono text-xs"
        />
        <Button type="submit" size="sm" variant="outline" disabled={!changed || setBase.isPending}>
          Save
        </Button>
      </form>
      <ErrorMessage error={setBase.error} />
    </li>
  );
}

/** Diagnostic project list. The real project UI arrives with workspaces in phases 5 and 6. */
export function ProjectList() {
  const projects = useProjects();
  const setAddProjectOpen = useUi((s) => s.setAddProjectOpen);

  return (
    <section className="flex flex-col gap-2">
      <div className="flex items-center justify-between">
        <h2 className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
          Projects
        </h2>
        <Button size="sm" variant="outline" onClick={() => setAddProjectOpen(true)}>
          Add project
        </Button>
      </div>
      <ErrorMessage error={projects.error} />
      {projects.data?.length === 0 ? (
        <p className="text-sm text-muted-foreground">No projects yet.</p>
      ) : null}
      <ul className="flex flex-col gap-2">
        {projects.data?.map((project) => (
          <ProjectRow key={project.id} project={project} />
        ))}
      </ul>
    </section>
  );
}
