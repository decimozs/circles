import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandShortcut,
} from "@circles/ui/components/command";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@circles/ui/components/dialog";
import { cn } from "@circles/ui/utils";
import {
  FolderIcon,
  GitBranchIcon,
  PlusIcon,
  SettingsIcon,
  SquareTerminalIcon,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router";
import { useSettingsPages } from "@/features/integrations/api";
import { useProjects } from "@/features/projects/api";
import { ProjectAvatar } from "@/features/projects/project-avatar";
import { useWorkspaces } from "@/features/workspaces/api";
import { StatusIndicator } from "@/features/workspaces/status";
import { flattenActions, useActionRegistry } from "@/lib/actions";
import { formatShortcut } from "@/lib/shortcuts";
import { formatAge } from "@/lib/time";
import { useUi } from "@/stores/ui";

type Tab = "all" | "workspaces" | "projects" | "actions" | "settings";

const TABS: { id: Tab; label: string }[] = [
  { id: "all", label: "All" },
  { id: "workspaces", label: "Workspaces" },
  { id: "projects", label: "Projects" },
  { id: "actions", label: "Actions" },
  { id: "settings", label: "Settings" },
];

const RECENT_COUNT = 5;

export function Palette() {
  const isOpen = useUi((s) => s.paletteOpen);
  const setOpen = useUi((s) => s.setPaletteOpen);
  const openCreate = useUi((s) => s.openCreate);
  const navigate = useNavigate();

  const [tab, setTab] = useState<Tab>("all");
  const [search, setSearch] = useState("");
  useEffect(() => {
    if (isOpen) {
      setTab("all");
      setSearch("");
    }
  }, [isOpen]);

  const projects = useProjects().data ?? [];
  const byProject = useMemo(() => new Map(projects.map((p) => [p.id, p])), [projects]);
  const workspaces = (useWorkspaces().data ?? []).filter((w) => w.state !== "archived");
  const sources = useActionRegistry((s) => s.sources);
  const actions = flattenActions(sources);

  const recent = [...workspaces]
    .sort((a, b) => (b.last_opened_at ?? b.updated_at) - (a.last_opened_at ?? a.updated_at))
    .slice(0, RECENT_COUNT);

  const searching = search.trim() !== "";
  const showRecent = tab === "all" && !searching;
  const showWorkspaces = tab === "workspaces" || (tab === "all" && searching);
  const showProjects = tab === "projects" || (tab === "all" && searching);
  const showActions = tab === "actions" || tab === "all";
  const settingsPages = useSettingsPages();
  const showSettings = tab === "settings" || (tab === "all" && searching);

  /** Close first so focus returns to the page, then run what was picked. */
  const run = (fn: () => void) => {
    setOpen(false);
    window.setTimeout(fn, 0);
  };

  const workspaceItem = (id: string, prefix: string) => {
    const w = workspaces.find((x) => x.id === id);
    if (!w) return null;
    const project = byProject.get(w.project_id);
    return (
      <CommandItem
        key={`${prefix}-${w.id}`}
        value={`${prefix} ${w.title ?? w.city} ${project?.name ?? ""} ${w.branch} ${w.id}`}
        onSelect={() => run(() => navigate(`/workspace/${w.id}`))}
      >
        {project ? <ProjectAvatar project={project} className="size-4" /> : <GitBranchIcon />}
        <span className="truncate">{w.title ?? w.city}</span>
        <StatusIndicator state={w.state} className="ml-1" />
        <CommandShortcut>{formatAge(w.last_opened_at ?? w.updated_at)}</CommandShortcut>
      </CommandItem>
    );
  };

  return (
    <Dialog open={isOpen} onOpenChange={(o) => setOpen(o)}>
      <DialogContent
        showCloseButton={false}
        className="top-[18%] translate-y-0 gap-0 overflow-hidden rounded-xl p-0 sm:max-w-2xl"
      >
        <DialogTitle className="sr-only">Command palette</DialogTitle>
        <DialogDescription className="sr-only">
          Search workspaces, projects, actions and settings.
        </DialogDescription>
        <Command>
          <CommandInput
            value={search}
            onValueChange={setSearch}
            placeholder="Type a command or search..."
          />
          <div role="tablist" aria-label="Filter results" className="flex gap-1 px-3 pt-3 pb-2">
            {TABS.map((t) => (
              <button
                key={t.id}
                type="button"
                role="tab"
                aria-selected={tab === t.id}
                onClick={() => setTab(t.id)}
                className={cn(
                  "rounded-md px-2.5 py-1 text-xs outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring/50",
                  tab === t.id
                    ? "bg-muted font-medium text-foreground"
                    : "text-muted-foreground hover:text-foreground",
                )}
              >
                {t.label}
              </button>
            ))}
          </div>

          <CommandList className="max-h-96">
            <CommandEmpty>No results.</CommandEmpty>

            {showRecent && recent.length > 0 ? (
              <CommandGroup heading="Recent workspaces">
                {recent.map((w) => workspaceItem(w.id, "recent"))}
              </CommandGroup>
            ) : null}

            {showWorkspaces && workspaces.length > 0 ? (
              <CommandGroup heading="Workspaces">
                {workspaces.map((w) => workspaceItem(w.id, "workspace"))}
              </CommandGroup>
            ) : null}

            {showProjects && projects.length > 0 ? (
              <CommandGroup heading="Projects">
                {projects.map((p) => (
                  <CommandItem
                    key={p.id}
                    value={`project ${p.name} new workspace ${p.id}`}
                    onSelect={() => run(() => openCreate(p.id))}
                  >
                    <ProjectAvatar project={p} className="size-4" />
                    <span className="truncate">{p.name}</span>
                    <CommandShortcut>New workspace</CommandShortcut>
                  </CommandItem>
                ))}
              </CommandGroup>
            ) : null}

            {showActions && actions.length > 0 ? (
              <CommandGroup heading="Actions">
                {actions.map((a) => (
                  <CommandItem
                    key={a.id}
                    value={`action ${a.title} ${a.keywords?.join(" ") ?? ""} ${a.id}`}
                    onSelect={() => run(a.run)}
                  >
                    {a.id.startsWith("project.") ||
                    a.id.startsWith("workspace.") ||
                    a.id.startsWith("section.") ? (
                      <PlusIcon />
                    ) : a.group === "Terminal" ? (
                      <SquareTerminalIcon />
                    ) : (
                      <FolderIcon />
                    )}
                    <span className="truncate">{a.title}</span>
                    {a.shortcuts?.[0] ? (
                      <CommandShortcut>{formatShortcut(a.shortcuts[0])}</CommandShortcut>
                    ) : null}
                  </CommandItem>
                ))}
              </CommandGroup>
            ) : null}

            {showSettings ? (
              <CommandGroup heading="Settings">
                {settingsPages.map((page) => (
                  <CommandItem
                    key={page.id}
                    value={`settings ${page.title} ${page.group}`}
                    onSelect={() => run(() => navigate(`/settings/${page.id}`))}
                  >
                    <SettingsIcon />
                    <span className="truncate">{page.title}</span>
                    <CommandShortcut>{page.group}</CommandShortcut>
                  </CommandItem>
                ))}
              </CommandGroup>
            ) : null}
          </CommandList>
        </Command>
      </DialogContent>
    </Dialog>
  );
}
