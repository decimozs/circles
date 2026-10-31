import type { Project } from "@circles/bindings/Project";
import type { Workspace } from "@circles/bindings/Workspace";
import { Button } from "@circles/ui/components/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@circles/ui/components/dropdown-menu";
import { ScrollArea } from "@circles/ui/components/scroll-area";
import { cn } from "@circles/ui/utils";
import {
  BotIcon,
  ChartColumnIcon,
  ChevronDownIcon,
  ChevronRightIcon,
  CircleHelpIcon,
  FolderPlusIcon,
  HomeIcon,
  PinIcon,
  PlusIcon,
  SearchIcon,
  SettingsIcon,
} from "lucide-react";
import { useState } from "react";
import { NavLink, useMatch, useNavigate } from "react-router";
import { ComposerOrb } from "@/components/composer-orb";
import { IconButton } from "@/components/shell/icon-button";
import { ShellControls } from "@/components/shell/shell-controls";
import { useTheme } from "@/components/theme-provider";
import { ThemeToggle } from "@/components/theme-toggle";
import { useMachineName } from "@/features/app/api";
import { useApprovals, useComposers, useWorkspaceLinks } from "@/features/composers/api";
import {
  ComposersSection,
  hasPinnedComposers,
  PinnedComposers,
} from "@/features/composers/sidebar-section";
import { useProjects } from "@/features/projects/api";
import { ProjectAvatar } from "@/features/projects/project-avatar";
import { MissingFolderMarker, ProjectMenu } from "@/features/projects/project-menu";
import { useBranchStates, useSections, useWorkspaces } from "@/features/workspaces/api";
import { StatusIndicator } from "@/features/workspaces/status";
import { WorkspaceActions } from "@/features/workspaces/workspace-actions";
import { formatAge } from "@/lib/time";
import { useChat } from "@/stores/chat";
import { useUi } from "@/stores/ui";

const NAV_ITEM =
  "flex h-8 w-full items-center gap-2 rounded-md px-2 text-sm text-sidebar-foreground outline-none transition-colors hover:bg-sidebar-accent focus-visible:ring-2 focus-visible:ring-sidebar-ring/60";

/** The last time the workspace was opened, or changed when it never was. */
const lastUsed = (w: Workspace) => w.last_opened_at ?? w.updated_at;

function WorkspaceRow({ workspace }: { workspace: Workspace }) {
  const unread = useChat((s) => s.unread[workspace.id] === true);
  const link = useWorkspaceLinks().get(workspace.id);
  const branch = useBranchStates().data?.get(workspace.id);
  const navigate = useNavigate();
  const viewing = useMatch(`/workspace/${workspace.id}`) !== null;
  return (
    <div className="group/row relative">
      <NavLink
        to={`/workspace/${workspace.id}`}
        className={({ isActive }) =>
          cn(
            "relative flex h-8 in-data-[density=compact]:h-6 items-center gap-2 rounded-md pr-2 pl-2 text-sm outline-none transition-colors",
            "hover:bg-sidebar-accent focus-visible:ring-2 focus-visible:ring-sidebar-ring/60",
            isActive
              ? "bg-sidebar-accent text-sidebar-accent-foreground"
              : "text-sidebar-foreground",
          )
        }
      >
        {({ isActive }) => (
          <>
            {isActive ||
            workspace.state === "working" ||
            workspace.state === "blocked" ||
            workspace.state === "errored" ? (
              <span
                data-testid="status-line"
                className={cn(
                  "absolute top-1.5 bottom-1.5 left-0 w-0.5 rounded-full",
                  workspace.state === "working"
                    ? "bg-success"
                    : workspace.state === "blocked"
                      ? "bg-warning"
                      : workspace.state === "errored"
                        ? "bg-destructive"
                        : "bg-foreground",
                )}
              />
            ) : null}
            <StatusIndicator state={workspace.state} spinner={false} plain branch={branch} />
            <span
              className={cn("min-w-0 flex-1 truncate", unread && "font-semibold text-foreground")}
            >
              {workspace.title ?? workspace.city}
            </span>
            {link ? (
              <ComposerOrb
                seed={link.composer_id}
                label={`Started by ${link.composer_name}`}
                className="size-3"
              />
            ) : null}
            {unread ? (
              <span
                role="img"
                aria-label="Unread"
                className="size-1.5 shrink-0 rounded-full bg-primary"
              />
            ) : null}
            <span
              title="Last used"
              className="shrink-0 text-2xs text-muted-foreground group-focus-within/row:opacity-0 group-hover/row:opacity-0"
            >
              {formatAge(lastUsed(workspace))}
            </span>
          </>
        )}
      </NavLink>
      <div className="absolute top-1 right-1 opacity-0 transition-opacity group-focus-within/row:opacity-100 group-hover/row:opacity-100 has-[[aria-expanded=true]]:opacity-100">
        <WorkspaceActions
          workspace={workspace}
          label={`Menu for ${workspace.title ?? workspace.city}`}
          className="size-6"
          onDeleted={() => {
            if (viewing) navigate("/");
          }}
        />
      </div>
    </div>
  );
}

function ProjectGroup({ project, workspaces }: { project: Project; workspaces: Workspace[] }) {
  const [open, setOpen] = useState(true);
  const openCreate = useUi((s) => s.openCreate);
  const Chevron = open ? ChevronDownIcon : ChevronRightIcon;

  return (
    <div className="flex flex-col gap-0.5">
      <div className="group/project flex h-8 in-data-[density=compact]:h-6 items-center gap-1 rounded-md pr-1 pl-2 hover:bg-sidebar-accent">
        <button
          type="button"
          onClick={() => setOpen((o) => !o)}
          aria-expanded={open}
          className="flex min-w-0 flex-1 items-center gap-2 text-left text-sm font-medium outline-none"
        >
          <ProjectAvatar project={project} className="size-4" />
          <span className="truncate">{project.name}</span>
          <span className="text-xs text-muted-foreground">{project.workspace_count}</span>
          <MissingFolderMarker project={project} />
          <Chevron className="size-3 text-muted-foreground opacity-0 group-hover/project:opacity-100" />
        </button>
        <ProjectMenu project={project} />
        <IconButton
          label={`New workspace in ${project.name}`}
          className="size-6"
          onClick={() => openCreate(project.id)}
        >
          <PlusIcon />
        </IconButton>
      </div>
      {open ? (
        <div className="flex flex-col gap-0.5 pl-3">
          {workspaces.map((w) => (
            <WorkspaceRow key={w.id} workspace={w} />
          ))}
        </div>
      ) : null}
    </div>
  );
}

export function Sidebar() {
  const navigate = useNavigate();
  const onHome = useMatch("/");
  const setShortcutsOpen = useUi((s) => s.setShortcutsOpen);
  const setAddProjectOpen = useUi((s) => s.setAddProjectOpen);
  const setUsageOpen = useUi((s) => s.setUsageOpen);
  const openCreate = useUi((s) => s.openCreate);
  const setPaletteOpen = useUi((s) => s.setPaletteOpen);
  const machineName = useMachineName();
  const { resolvedTheme, setTheme } = useTheme();

  const projects = useProjects().data ?? [];
  const sections = useSections().data ?? [];
  const live = (useWorkspaces().data ?? []).filter((w) => w.state !== "archived");
  const composers = useComposers().data ?? [];
  const waiting = (useApprovals({ onlyPending: true }).data ?? []).length;
  const onComposers = useMatch("/composers");

  const pinned = live.filter((w) => w.pinned);
  const unpinned = live.filter((w) => !w.pinned);
  const inSection = (sectionId: string) => unpinned.filter((w) => w.section_id === sectionId);
  const loose = (projectId: string) =>
    unpinned.filter((w) => w.project_id === projectId && !w.section_id);

  return (
    <div className="flex h-full flex-col bg-sidebar text-sidebar-foreground">
      <div className="relative z-30 flex h-10 shrink-0 items-center justify-end pr-2">
        <ShellControls />
      </div>

      <div className="px-2 pb-2">
        <DropdownMenu>
          <DropdownMenuTrigger
            render={
              <button
                type="button"
                className="flex h-8 w-full items-center gap-2 rounded-md px-2 text-sm font-medium outline-none hover:bg-sidebar-accent focus-visible:ring-2 focus-visible:ring-sidebar-ring/60"
              />
            }
          >
            <span className="flex size-5 items-center justify-center rounded-full bg-muted text-2xs">
              {machineName.slice(0, 1).toUpperCase()}
            </span>
            <span className="min-w-0 flex-1 truncate text-left">{machineName}</span>
            <ChevronDownIcon className="size-3.5 text-muted-foreground" />
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="w-56">
            <DropdownMenuItem onClick={() => navigate("/settings/general")}>
              Settings
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => setShortcutsOpen(true)}>
              Keyboard shortcuts
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => setTheme(resolvedTheme === "dark" ? "light" : "dark")}>
              Toggle theme
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <div className="flex items-center justify-between px-2 py-1.5">
              <span className="text-xs text-muted-foreground">Theme</span>
              <ThemeToggle />
            </div>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>

      <nav className="flex flex-col gap-0.5 px-2" aria-label="Primary">
        <button type="button" className={NAV_ITEM} onClick={() => setPaletteOpen(true)}>
          <SearchIcon className="size-4 text-muted-foreground" />
          Search
        </button>
        <button type="button" className={NAV_ITEM} onClick={() => openCreate()}>
          <PlusIcon className="size-4 text-muted-foreground" />
          Create
        </button>
        <NavLink
          to="/"
          end
          className={cn(NAV_ITEM, onHome && "bg-sidebar-accent text-sidebar-accent-foreground")}
        >
          <HomeIcon className="size-4 text-muted-foreground" />
          Home
        </NavLink>
        <NavLink
          to="/composers"
          end
          className={cn(
            NAV_ITEM,
            onComposers && "bg-sidebar-accent text-sidebar-accent-foreground",
          )}
        >
          <BotIcon className="size-4 text-muted-foreground" />
          Composer
          {waiting > 0 ? (
            <span
              role="status"
              aria-label={`${waiting} waiting for you`}
              className="ml-auto rounded-full bg-warning/20 px-1.5 text-xs text-warning"
            >
              {waiting}
            </span>
          ) : null}
        </NavLink>
      </nav>

      <div className="my-2 border-t border-sidebar-border" />

      <ScrollArea className="min-h-0 flex-1">
        <div className="flex flex-col gap-3 px-2 pb-3">
          {pinned.length > 0 || hasPinnedComposers(composers) ? (
            <section aria-label="Pinned" className="flex flex-col gap-0.5">
              <h2 className="flex items-center gap-1 px-2 py-1 text-xs text-muted-foreground">
                <PinIcon className="size-3" />
                Pinned
              </h2>
              <PinnedComposers />
              {pinned.map((w) => (
                <WorkspaceRow key={w.id} workspace={w} />
              ))}
            </section>
          ) : null}

          {sections.map((section) => {
            const items = inSection(section.id);
            if (items.length === 0) return null;
            return (
              <section key={section.id} aria-label={section.name} className="flex flex-col gap-0.5">
                <h2 className="px-2 py-1 text-xs text-muted-foreground">{section.name}</h2>
                {items.map((w) => (
                  <WorkspaceRow key={w.id} workspace={w} />
                ))}
              </section>
            );
          })}

          <ComposersSection />

          <section aria-label="My workspaces" className="flex flex-col gap-0.5">
            {projects.length === 0 ? (
              // Nothing to call "My workspaces" yet, so the heading is the invitation.
              <div className="flex flex-col items-start gap-2 px-2 py-1">
                <h2 className="text-xs text-muted-foreground">Add a repository to get started.</h2>
                <Button size="sm" variant="outline" onClick={() => setAddProjectOpen(true)}>
                  <FolderPlusIcon />
                  Add project
                </Button>
              </div>
            ) : (
              <>
                <h2 className="px-2 py-1 text-xs text-muted-foreground">My workspaces</h2>
                {projects.map((project) => (
                  <ProjectGroup key={project.id} project={project} workspaces={loose(project.id)} />
                ))}
              </>
            )}
          </section>
        </div>
      </ScrollArea>

      <div className="flex h-11 shrink-0 items-center justify-between border-t border-sidebar-border px-3">
        <span className="rounded border border-dashed border-border px-1.5 py-0.5 font-mono text-2xs text-muted-foreground">
          v0.1.0
        </span>
        <div className="flex items-center gap-0.5">
          <IconButton label="Usage" onClick={() => setUsageOpen(true)}>
            <ChartColumnIcon />
          </IconButton>
          <IconButton label="Keyboard shortcuts" hint="⌘/" onClick={() => setShortcutsOpen(true)}>
            <CircleHelpIcon />
          </IconButton>
          <IconButton label="Settings" onClick={() => navigate("/settings/general")}>
            <SettingsIcon />
          </IconButton>
        </div>
      </div>
    </div>
  );
}
