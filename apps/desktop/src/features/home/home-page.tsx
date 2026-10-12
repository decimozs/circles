import type { Project } from "@circles/bindings/Project";
import type { Workspace } from "@circles/bindings/Workspace";
import { Button } from "@circles/ui/components/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@circles/ui/components/dropdown-menu";
import { Input } from "@circles/ui/components/input";
import {
  ArchiveIcon,
  ChevronDownIcon,
  CloudDownloadIcon,
  FolderOpenIcon,
  GitBranchIcon,
  SearchIcon,
  SparklesIcon,
} from "lucide-react";
import { useMemo, useState } from "react";
import { useNavigate } from "react-router";
import { useMachineName } from "@/features/app/api";
import { useProjects } from "@/features/projects/api";
import { ProjectAvatar } from "@/features/projects/project-avatar";
import { useWorkspaces } from "@/features/workspaces/api";
import { StatusIndicator } from "@/features/workspaces/status";
import { formatAge, groupByRecency } from "@/lib/time";
import { useUi } from "@/stores/ui";

function matches(workspace: Workspace, project: Project | undefined, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  return [workspace.title, workspace.city, workspace.branch, project?.name]
    .filter(Boolean)
    .some((field) => (field as string).toLowerCase().includes(q));
}

function WorkspaceItem({ workspace, project }: { workspace: Workspace; project?: Project }) {
  const navigate = useNavigate();
  return (
    <li>
      <button
        type="button"
        onClick={() => navigate(`/workspace/${workspace.id}`)}
        className="flex h-10 w-full items-center gap-3 rounded-lg px-3 text-left text-sm outline-none transition-colors hover:bg-muted focus-visible:bg-muted"
      >
        {project ? <ProjectAvatar project={project} className="size-5" /> : null}
        <span className="min-w-0 flex-1 truncate">{workspace.title ?? workspace.city}</span>
        {workspace.state === "archived" ? (
          <ArchiveIcon aria-label="Archived" className="size-3.5 shrink-0 text-muted-foreground" />
        ) : (
          <StatusIndicator state={workspace.state} />
        )}
        <span className="hidden max-w-40 shrink-0 truncate text-xs text-muted-foreground sm:block">
          {project?.name}
        </span>
        <span className="w-10 shrink-0 text-right text-xs text-muted-foreground">
          {formatAge(workspace.updated_at)}
        </span>
      </button>
    </li>
  );
}

function FirstRun() {
  const setAddProjectOpen = useUi((s) => s.setAddProjectOpen);
  const cards = [
    {
      tab: "local" as const,
      icon: FolderOpenIcon,
      title: "Add project",
      text: "Open a repository that is already on this Mac.",
    },
    {
      tab: "github" as const,
      icon: CloudDownloadIcon,
      title: "Clone from GitHub",
      text: "Pick one of your repositories and clone it.",
    },
    {
      tab: "quick" as const,
      icon: SparklesIcon,
      title: "Quick start",
      text: "Create a new repository and start from scratch.",
    },
  ];

  return (
    <div className="flex flex-col items-center gap-6 py-16 text-center">
      <div>
        <h2 className="text-lg font-semibold">Add your first project</h2>
        <p className="mt-1 text-sm text-muted-foreground">
          Every workspace is an isolated copy of a project, so start by adding one.
        </p>
      </div>
      <div className="grid w-full max-w-2xl gap-3 sm:grid-cols-3">
        {cards.map(({ tab, icon: Icon, title, text }) => (
          <button
            key={tab}
            type="button"
            onClick={() => setAddProjectOpen(true, tab)}
            className="flex flex-col items-start gap-2 rounded-xl border border-border p-4 text-left outline-none transition-colors hover:bg-muted focus-visible:bg-muted"
          >
            <Icon className="size-5 text-muted-foreground" />
            <span className="text-sm font-medium">{title}</span>
            <span className="text-xs text-muted-foreground">{text}</span>
          </button>
        ))}
      </div>
    </div>
  );
}

export function HomePage() {
  const machineName = useMachineName();
  const projects = useProjects();
  const workspaces = useWorkspaces();
  const openCreate = useUi((s) => s.openCreate);
  const [query, setQuery] = useState("");
  const [showArchived, setShowArchived] = useState(false);

  const projectList = projects.data ?? [];
  const byProject = useMemo(() => new Map(projectList.map((p) => [p.id, p])), [projectList]);
  const all = workspaces.data ?? [];
  const visible = useMemo(
    () =>
      all
        .filter((w) => showArchived || w.state !== "archived")
        .filter((w) => matches(w, byProject.get(w.project_id), query)),
    [all, showArchived, query, byProject],
  );
  const groups = useMemo(() => groupByRecency(visible, (w) => w.updated_at), [visible]);

  const loading = projects.isLoading || workspaces.isLoading;
  const noProjects = !loading && projectList.length === 0;
  const noWorkspaces =
    !loading &&
    !noProjects &&
    all.filter((w) => showArchived || w.state !== "archived").length === 0;

  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col px-6 pt-4 pb-10">
      <header className="flex items-center justify-between gap-4 py-2">
        <h1 className="flex min-w-0 items-center gap-2.5 text-lg font-medium">
          <span className="flex size-6 shrink-0 items-center justify-center rounded-full bg-muted text-xs">
            {machineName.slice(0, 1).toUpperCase()}
          </span>
          <span className="truncate">{machineName}</span>
        </h1>
        <Button size="sm" onClick={() => openCreate()}>
          Create workspace
        </Button>
      </header>

      {noProjects ? (
        <FirstRun />
      ) : (
        <>
          <div className="flex items-center gap-2 border-b border-border pb-2">
            <div className="relative flex-1">
              <SearchIcon className="pointer-events-none absolute top-2.5 left-2 size-4 text-muted-foreground" />
              <Input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Search"
                aria-label="Search workspaces"
                className="h-9 border-0 bg-transparent pl-8 shadow-none focus-visible:ring-0 dark:bg-transparent"
              />
            </div>
            <DropdownMenu>
              <DropdownMenuTrigger
                render={
                  <Button variant="ghost" size="sm" className="gap-1.5 text-muted-foreground" />
                }
              >
                <ArchiveIcon />
                {showArchived ? "Showing archived" : "Hiding archived"}
                <ChevronDownIcon className="size-3" />
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="w-48">
                <DropdownMenuRadioGroup
                  value={showArchived ? "show" : "hide"}
                  onValueChange={(v) => setShowArchived(v === "show")}
                >
                  <DropdownMenuRadioItem value="hide">Hide archived</DropdownMenuRadioItem>
                  <DropdownMenuRadioItem value="show">Show archived</DropdownMenuRadioItem>
                </DropdownMenuRadioGroup>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>

          {noWorkspaces ? (
            <div className="flex flex-col items-center gap-3 py-16 text-center">
              <GitBranchIcon className="size-6 text-muted-foreground" />
              <p className="text-sm text-muted-foreground">No workspaces yet.</p>
              <Button size="sm" onClick={() => openCreate()}>
                Create your first workspace
              </Button>
            </div>
          ) : groups.length === 0 && !loading ? (
            <p className="py-16 text-center text-sm text-muted-foreground">
              {query.trim() ? "No workspaces match your search." : "Nothing to show."}
            </p>
          ) : (
            <div className="flex flex-col gap-4 pt-3">
              {groups.map(({ group, items }) => (
                <section key={group} aria-label={group}>
                  <h2 className="flex items-center justify-between px-3 pb-1 text-xs font-medium text-muted-foreground">
                    {group}
                    <span className="font-normal tabular-nums">{items.length}</span>
                  </h2>
                  <ul className="flex flex-col">
                    {items.map((w) => (
                      <WorkspaceItem
                        key={w.id}
                        workspace={w}
                        project={byProject.get(w.project_id)}
                      />
                    ))}
                  </ul>
                </section>
              ))}
            </div>
          )}
        </>
      )}
    </div>
  );
}
