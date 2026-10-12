import type { Section } from "@circles/bindings/Section";
import type { Workspace } from "@circles/bindings/Workspace";
import { Button } from "@circles/ui/components/button";
import { Input } from "@circles/ui/components/input";
import { Loader2Icon, PinIcon, PlusIcon, Trash2Icon } from "lucide-react";
import { useState } from "react";
import { useNavigate } from "react-router";
import { useProjects } from "@/features/projects/api";
import { ErrorMessage } from "@/features/projects/error-message";
import {
  useCreateSection,
  useDeleteSection,
  useRenameSection,
  useSections,
  useSetWorkspaceSection,
  useWorkspaces,
} from "@/features/workspaces/api";
import { useQuickCreateWorkspace } from "@/features/workspaces/create-workspace";
import { STATE_LABEL, StatusIndicator } from "@/features/workspaces/status";
import { WorkspaceActions } from "@/features/workspaces/workspace-actions";
import { formatAge } from "@/lib/time";

function SectionRow({ section }: { section: Section }) {
  const rename = useRenameSection();
  const remove = useDeleteSection();
  const [name, setName] = useState(section.name);

  return (
    <li className="flex flex-col gap-1">
      <form
        className="flex items-center gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          rename.mutate({ id: section.id, name });
        }}
      >
        <Input
          value={name}
          onChange={(e) => setName(e.target.value)}
          aria-label={`Section name ${section.name}`}
          className="h-7 w-48 text-xs"
        />
        <Button
          type="submit"
          size="sm"
          variant="outline"
          disabled={name.trim() === section.name || rename.isPending}
        >
          Rename
        </Button>
        <Button
          type="button"
          size="icon-sm"
          variant="ghost"
          aria-label={`Delete section ${section.name}`}
          className="text-muted-foreground"
          onClick={() => remove.mutate(section.id)}
        >
          <Trash2Icon />
        </Button>
      </form>
      <ErrorMessage error={rename.error ?? remove.error} />
    </li>
  );
}

function Sections() {
  const sections = useSections().data ?? [];
  const create = useCreateSection();
  const [name, setName] = useState("");

  return (
    <div className="flex flex-col gap-2">
      <h3 className="text-xs text-muted-foreground">Sections</h3>
      <ul className="flex flex-col gap-1.5">
        {sections.map((section) => (
          <SectionRow key={section.id} section={section} />
        ))}
      </ul>
      <form
        className="flex items-center gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          create.mutate(name, { onSuccess: () => setName("") });
        }}
      >
        <Input
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="New section"
          aria-label="New section name"
          className="h-7 w-48 text-xs"
        />
        <Button
          type="submit"
          size="sm"
          variant="outline"
          disabled={!name.trim() || create.isPending}
        >
          Add section
        </Button>
      </form>
      <ErrorMessage error={create.error} />
    </div>
  );
}

function WorkspaceRow({ workspace }: { workspace: Workspace }) {
  const navigate = useNavigate();
  const sections = useSections().data ?? [];
  const setSection = useSetWorkspaceSection();
  const canOpen = workspace.state !== "archived";

  return (
    <li className="flex flex-col gap-1.5 rounded-md border border-border p-3">
      <div className="flex items-center gap-2">
        <StatusIndicator state={workspace.state} />
        {canOpen ? (
          <button
            type="button"
            className="font-medium underline-offset-4 outline-none hover:underline focus-visible:underline"
            onClick={() => navigate(`/workspace/${workspace.id}`)}
          >
            {workspace.city}
          </button>
        ) : (
          <span className="font-medium">{workspace.city}</span>
        )}
        {workspace.pinned ? <PinIcon className="size-3 text-muted-foreground" /> : null}
        <span className="text-xs text-muted-foreground">
          {STATE_LABEL[workspace.state]} · {formatAge(workspace.updated_at)}
        </span>
        <div className="ml-auto flex items-center gap-1">
          <select
            aria-label={`Section for ${workspace.city}`}
            value={workspace.section_id ?? ""}
            onChange={(e) =>
              setSection.mutate({ workspaceId: workspace.id, sectionId: e.target.value || null })
            }
            className="h-7 rounded-md border border-input bg-background px-1.5 text-xs"
          >
            <option value="">No section</option>
            {sections.map((s) => (
              <option key={s.id} value={s.id}>
                {s.name}
              </option>
            ))}
          </select>
          <WorkspaceActions workspace={workspace} />
        </div>
      </div>
      <p className="font-mono text-xs break-all text-muted-foreground">
        {workspace.branch} from {workspace.base_branch}
      </p>
      <p className="font-mono text-xs break-all text-muted-foreground">{workspace.path}</p>
      {workspace.error ? <p className="text-xs text-destructive">{workspace.error}</p> : null}
    </li>
  );
}

/** Diagnostic workspace list. The real workspace UI arrives with the Home screen in phase 6. */
export function WorkspaceList() {
  const projects = useProjects().data ?? [];
  const workspaces = useWorkspaces();
  const { create, isPending } = useQuickCreateWorkspace();

  return (
    <section className="flex flex-col gap-3">
      <h2 className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
        Workspaces
      </h2>
      <ErrorMessage error={workspaces.error} />
      {projects.length === 0 ? (
        <p className="text-sm text-muted-foreground">Add a project to create workspaces.</p>
      ) : null}
      {projects.map((project) => {
        const items = (workspaces.data ?? []).filter((w) => w.project_id === project.id);
        return (
          <div key={project.id} className="flex flex-col gap-2">
            <div className="flex items-center gap-2">
              <h3 className="text-sm font-medium">{project.name}</h3>
              <Button
                size="sm"
                variant="outline"
                disabled={isPending}
                onClick={() => create(project)}
              >
                {isPending ? <Loader2Icon className="animate-spin" /> : <PlusIcon />}
                New workspace
              </Button>
            </div>
            {items.length === 0 ? (
              <p className="text-xs text-muted-foreground">No workspaces yet.</p>
            ) : null}
            <ul className="flex flex-col gap-2">
              {items.map((w) => (
                <WorkspaceRow key={w.id} workspace={w} />
              ))}
            </ul>
          </div>
        );
      })}
      <Sections />
    </section>
  );
}
