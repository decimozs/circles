import { Input } from "@circles/ui/components/input";
import { cn } from "@circles/ui/utils";
import {
  ArrowLeftIcon,
  BotIcon,
  ChevronDownIcon,
  CircleUserIcon,
  CpuIcon,
  FlaskConicalIcon,
  GitBranchIcon,
  HardDriveIcon,
  KeyRoundIcon,
  PaletteIcon,
  PlugIcon,
  SearchIcon,
  SlidersHorizontalIcon,
  SparklesIcon,
  WorkflowIcon,
} from "lucide-react";
import { type ReactNode, useEffect, useRef } from "react";
import { NavLink, useNavigate } from "react-router";
import { JiraIcon } from "@/components/jira-icon";
import { useMachineName } from "@/features/app/api";
import { useSettingsPages } from "@/features/integrations/api";
import { useProjects } from "@/features/projects/api";
import { ProjectAvatar } from "@/features/projects/project-avatar";
import { useSettingsSearch } from "./search-store";

const ICONS: Record<string, ReactNode> = {
  general: <SlidersHorizontalIcon />,
  account: <CircleUserIcon />,
  appearance: <PaletteIcon />,
  models: <CpuIcon />,
  git: <GitBranchIcon />,
  agents: <BotIcon />,
  environment: <KeyRoundIcon />,
  composers: <WorkflowIcon />,
  integrations: <PlugIcon />,
  smart: <SparklesIcon />,
  jira: <JiraIcon />,
  experimental: <FlaskConicalIcon />,
  advanced: <HardDriveIcon />,
};

const GROUPS = ["Personal", "Agents & environment", "Integrations", "More"] as const;

const linkClass = ({ isActive }: { isActive: boolean }) =>
  cn(
    "flex h-8 items-center gap-2.5 rounded-md px-2 text-sm outline-none transition-colors focus-visible:ring-2 focus-visible:ring-sidebar-ring/60 [&_svg]:size-4 [&_svg]:shrink-0",
    isActive
      ? "bg-sidebar-accent font-medium text-sidebar-foreground"
      : "text-muted-foreground hover:bg-sidebar-accent/60 hover:text-sidebar-foreground",
  );

/** What the left pane shows while the settings are open. */
export function SettingsNav() {
  const navigate = useNavigate();
  const projects = useProjects().data ?? [];
  const machineName = useMachineName();
  const pages = useSettingsPages();
  const { query, setQuery } = useSettingsSearch();
  const search = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "f") {
        event.preventDefault();
        search.current?.focus();
        search.current?.select();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
  // Leaving the settings forgets the search.
  useEffect(() => () => useSettingsSearch.getState().setQuery(""), []);

  return (
    <nav
      aria-label="Settings"
      className="flex h-full flex-col gap-1 overflow-y-auto bg-sidebar px-2 pb-3 text-sidebar-foreground"
    >
      <div className="relative z-30 flex h-10 shrink-0 items-center justify-end">
        <button
          type="button"
          aria-label="Back"
          title="Back"
          onClick={() => navigate("/")}
          className="flex h-7 items-center gap-1.5 rounded-md px-2 text-sm text-muted-foreground outline-none hover:bg-sidebar-accent hover:text-sidebar-foreground focus-visible:ring-2 focus-visible:ring-sidebar-ring/60"
        >
          <ArrowLeftIcon className="size-4" />
        </button>
      </div>
      <div className="flex -mt-1 h-8 items-center gap-2 px-2 text-sm font-medium">
        <span className="flex size-5 items-center justify-center rounded-full bg-muted text-2xs">
          {machineName.slice(0, 1).toUpperCase()}
        </span>
        <span className="min-w-0 flex-1 truncate">{machineName}</span>
        <ChevronDownIcon className="size-3.5 text-muted-foreground" />
      </div>
      <div className="relative my-2">
        <SearchIcon className="pointer-events-none absolute top-2.5 left-2.5 size-3.5 text-muted-foreground" />
        <Input
          ref={search}
          type="search"
          aria-label="Search settings"
          placeholder="Search settings"
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
          }}
          className="h-9 pr-12 pl-8 text-sm"
        />
        <kbd className="pointer-events-none absolute top-2 right-2 rounded border border-border px-1 text-2xs text-muted-foreground">
          ⌘ F
        </kbd>
      </div>
      {GROUPS.filter((group) => pages.some((p) => p.group === group)).map((group) => (
        <div key={group} className="mt-2 flex flex-col gap-0.5">
          <h2 className="px-2 py-1 text-xs text-muted-foreground">{group}</h2>
          {pages
            .filter((p) => p.group === group)
            .map((p) => (
              <NavLink
                key={p.id}
                to={`/settings/${p.id}`}
                className={linkClass}
                onClick={() => setQuery("")}
              >
                {ICONS[p.id]}
                {p.title}
              </NavLink>
            ))}
          {group === "Agents & environment" && projects.length > 0 ? (
            <div className="mt-4 flex flex-col gap-0.5">
              <h2 className="px-2 py-1 text-xs text-muted-foreground">Repositories</h2>
              {projects.map((p) => (
                <NavLink
                  key={p.id}
                  to={`/settings/repo/${p.id}`}
                  className={linkClass}
                  onClick={() => setQuery("")}
                >
                  <ProjectAvatar project={p} className="size-4" />
                  <span className="truncate">{p.name}</span>
                </NavLink>
              ))}
            </div>
          ) : null}
        </div>
      ))}
    </nav>
  );
}
