import { Button } from "@circles/ui/components/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@circles/ui/components/dropdown-menu";
import { ChevronDownIcon } from "lucide-react";
import { Link, useMatch } from "react-router";
import { toast } from "sonner";
import { useJiraStatus } from "@/features/integrations/api";
import { useProjects } from "@/features/projects/api";
import { ipc } from "@/lib/ipc";
import { SETTINGS_PAGES } from "@/lib/settings-pages";
import { useSettingsSync, useSettingsView } from "./api";
import { searchSettings } from "./defs";
import { Row, SettingsGroupPage } from "./pages";
import { RepoSettings } from "./repo-page";
import { useSettingsSearch } from "./search-store";

export function OpenFileMenu({ projectId }: { projectId?: string }) {
  const open = (scope: "user" | "project" | "local") => async () => {
    try {
      await ipc.settingsOpenFile(scope, projectId);
    } catch (e) {
      toast.error(
        e instanceof Error
          ? e.message
          : ((e as { message?: string }).message ?? "Cannot open the file"),
      );
    }
  };
  return (
    <DropdownMenu>
      <DropdownMenuTrigger render={<Button variant="outline" size="sm" />}>
        Open settings.toml
        <ChevronDownIcon />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuItem onClick={open("user")}>User (every project)</DropdownMenuItem>
        {projectId ? (
          <>
            <DropdownMenuItem onClick={open("project")}>
              Project (shared in the repository)
            </DropdownMenuItem>
            <DropdownMenuItem onClick={open("local")}>
              Local (this Mac, this project)
            </DropdownMenuItem>
          </>
        ) : null}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** Which page of the settings the address shows. */
export function useSettingsRoute() {
  const splat = useMatch("/settings/*")?.params["*"] ?? "";
  const repoId = splat.startsWith("repo/") ? splat.slice(5) : undefined;
  return { repoId, pageId: repoId ? "repo" : splat || "general" };
}

export function SettingsPage() {
  const { repoId, pageId } = useSettingsRoute();
  const projects = useProjects().data ?? [];
  const view = useSettingsView(repoId);
  useSettingsSync(repoId);
  const query = useSettingsSearch((s) => s.query);

  const found = searchSettings(query);
  const jira = useJiraStatus().data;
  const page = SETTINGS_PAGES.find((p) => p.id === pageId);
  const repo = projects.find((p) => p.id === repoId);
  const title = query
    ? "Search results"
    : repoId
      ? (repo?.name ?? "Repository")
      : (page?.title ?? "Settings");

  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto max-w-3xl px-8 pt-10 pb-16">
        <div className="mb-6 flex items-center justify-between gap-4">
          <h1 className="text-2xl font-semibold">{title}</h1>
          <OpenFileMenu projectId={repoId} />
        </div>
        {view.error ? (
          <p role="alert" className="text-sm text-destructive">
            {(view.error as Error).message}
          </p>
        ) : null}
        {!view.data ? null : query ? (
          found.length === 0 ? (
            <p className="text-sm text-muted-foreground">Nothing matches “{query}”.</p>
          ) : (
            found.map((def) => <Row key={def.id} def={def} view={view.data} highlight={query} />)
          )
        ) : repoId ? (
          <RepoSettings projectId={repoId} />
        ) : page?.id === "jira" && jira && !jira.connected ? (
          <p className="text-sm text-muted-foreground">
            Jira is not connected. Connect it in{" "}
            <Link className="underline" to="/settings/integrations">
              Integrations
            </Link>
            .
          </p>
        ) : page ? (
          <SettingsGroupPage page={page.id} view={view.data} />
        ) : (
          <p className="text-sm text-muted-foreground">No such settings page.</p>
        )}
      </div>
    </div>
  );
}
