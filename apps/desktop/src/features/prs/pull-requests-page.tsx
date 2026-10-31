import { cn } from "@circles/ui/utils";
import { useQueryClient } from "@tanstack/react-query";
import { GitPullRequestIcon, Loader2Icon } from "lucide-react";
import { useMemo, useState } from "react";
import { Group, Panel, Separator } from "react-resizable-panels";
import { toast } from "sonner";
import { describeError } from "@/features/chat/use-session";
import { useProjects } from "@/features/projects/api";
import { useWorkspaces } from "@/features/workspaces/api";
import { usePanelSync } from "@/hooks/use-panel-sync";
import { useUserResizing } from "@/hooks/use-user-resizing";
import { formatAge } from "@/lib/time";
import { PR_PANE, useUi } from "@/stores/ui";
import { refreshPrList, usePrList } from "./api";
import { DetailPane } from "./detail-pane";
import { PrRow } from "./pr-row";
import { PrToolbar } from "./pr-toolbar";
import { groupRows, matches, parseQuery, sortRows } from "./query";
import { tabKey, usePrs } from "./store";

/** The line between the list and the pane, as the sidebars have. */
const SEPARATOR =
  "w-px bg-border outline-none transition-colors data-[separator=hover]:bg-ring data-[separator=active]:bg-ring focus-visible:bg-ring";

/** The project whose workspace was opened last, when the person has not chosen one. */
function lastWorkedIn(
  workspaces: { project_id: string; last_opened_at: number | null }[],
  projectIds: string[],
): string | null {
  const latest = [...workspaces]
    .filter((w) => projectIds.includes(w.project_id))
    .sort((a, b) => (b.last_opened_at ?? 0) - (a.last_opened_at ?? 0))[0];
  return latest?.project_id ?? null;
}

/** Pull requests of the repositories you have added, in one list. */
export function PullRequestsPage() {
  const queryClient = useQueryClient();
  const projectsQuery = useProjects();
  const workspacesQuery = useWorkspaces();
  const projects = projectsQuery.data ?? [];
  const workspaces = workspacesQuery.data ?? [];
  const { scope: chosen, which, sort, filters, query, tabs, selected } = usePrs();
  const { setScope, setWhich, setSort, setFilters, setQuery, openTab } = usePrs();
  // Something is open beside the list: the list narrows to a column.
  const split = tabs.length > 0;
  // The pane can be dragged wider or narrower, like the sidebars, and the width is remembered. A
  // size it takes by itself, when the window changes, is not remembered.
  const prPaneWidth = useUi.getState().prPaneWidth;
  const setPrPaneWidth = useUi((s) => s.setPrPaneWidth);
  const resizing = useUserResizing();
  const pane = usePanelSync({
    visible: split,
    canShow: split,
    // Dragging it shut is not how it is closed: its tabs are. It comes back to its smallest width.
    setOpen: () => {},
    setWidth: setPrPaneWidth,
    remember: resizing.isResizing,
  });

  const scope =
    chosen === undefined
      ? lastWorkedIn(
          workspaces,
          projects.map((p) => p.id),
        )
      : chosen;
  // Until the person has chosen, the project comes from their workspaces, so GitHub is not asked
  // for the wrong one while those load.
  const ready = chosen !== undefined || (projectsQuery.isSuccess && workspacesQuery.isSuccess);
  const list = usePrList(scope, which, ready);
  const [forcing, setForcing] = useState(false);
  const refreshing = forcing || list.isFetching || list.data?.refreshing === true;
  const fetchedAt = list.data?.fetched_at ?? null;
  const refreshNow = async () => {
    setForcing(true);
    try {
      await refreshPrList(queryClient, scope, which);
    } catch (e) {
      toast.error(describeError(e));
    } finally {
      setForcing(false);
    }
  };
  const showProject = scope === null;

  const groups = useMemo(() => {
    const parsed = parseQuery(query);
    const found = (list.data?.rows ?? []).filter((r) => matches(r, parsed, filters));
    return groupRows(sortRows(found, sort));
  }, [list.data, query, filters, sort]);
  const shown = groups.reduce((n, g) => n + g.rows.length, 0);

  return (
    <section className="h-full" aria-label="Pull requests">
      <Group orientation="horizontal" className="h-full">
        <Panel id="pr-list" minSize={320}>
          <div className="h-full overflow-y-auto">
            <div
              className={cn("mx-auto flex flex-col gap-4 px-6 pt-4 pb-10", !split && "max-w-4xl")}
            >
              <h1 className="text-base font-semibold">Pull requests</h1>
              <PrToolbar
                query={query}
                onQuery={setQuery}
                sort={sort}
                onSort={setSort}
                filters={filters}
                onFilters={setFilters}
                which={which}
                onWhich={setWhich}
                projects={projects}
                scope={scope}
                onScope={setScope}
                fetching={refreshing}
                onRefresh={refreshNow}
              />
              {fetchedAt !== null ? (
                <p role="status" className="-mt-2 text-xs text-muted-foreground">
                  {Date.now() - fetchedAt < 60_000
                    ? "Updated just now"
                    : `Updated ${formatAge(fetchedAt)} ago`}
                  {refreshing ? " · refreshing…" : ""}
                </p>
              ) : null}

              {(list.data?.failures ?? []).map((f) => (
                <p
                  key={f.project_id}
                  role="alert"
                  className="rounded-md border border-warning/40 bg-warning/5 px-3 py-2 text-xs"
                >
                  <strong className="font-medium">{f.project_name}:</strong> {f.message}
                  {f.fix ? (
                    <>
                      {" "}
                      Run <code className="rounded bg-muted px-1 font-mono">{f.fix}</code>.
                    </>
                  ) : null}
                </p>
              ))}

              {list.isPending ? (
                <p
                  role="status"
                  className="flex items-center gap-2 py-6 text-sm text-muted-foreground"
                >
                  <Loader2Icon className="size-4 animate-spin" />
                  Asking GitHub…
                </p>
              ) : list.isError ? (
                <p role="alert" className="py-6 text-sm text-destructive">
                  {String((list.error as { message?: string })?.message ?? list.error)}
                </p>
              ) : projects.length === 0 ? (
                <div className="flex flex-col items-center gap-1 py-16 text-center text-sm text-muted-foreground">
                  <GitPullRequestIcon className="mb-2 size-6" />
                  <p>Add a repository to see its pull requests.</p>
                </div>
              ) : shown === 0 ? (
                <div className="flex flex-col items-center gap-1 py-16 text-center text-sm text-muted-foreground">
                  <GitPullRequestIcon className="mb-2 size-6" />
                  <p>
                    {(list.data?.rows.length ?? 0) > 0
                      ? "No pull request matches."
                      : "No pull requests here."}
                  </p>
                </div>
              ) : (
                groups.map((group) => (
                  <section
                    key={group.id}
                    aria-label={group.title}
                    className="flex flex-col gap-0.5"
                  >
                    <h2 className="flex items-center gap-2 border-b border-border px-3 pb-1.5 text-xs text-muted-foreground">
                      {group.title}
                      <span className="tabular-nums">{group.rows.length}</span>
                    </h2>
                    {group.rows.map((row) => (
                      <PrRow
                        key={`${row.project_id}-${row.number}`}
                        row={row}
                        compact={split}
                        selected={selected === tabKey(row.project_id, row.number)}
                        onOpen={openTab}
                        showProject={showProject}
                      />
                    ))}
                  </section>
                ))
              )}
            </div>
          </div>
        </Panel>
        <Separator
          {...resizing.handlers}
          disabled={!split}
          className={cn(SEPARATOR, !split && "pointer-events-none opacity-0")}
        />
        <Panel
          id="pr-pane"
          panelRef={pane.ref}
          collapsible
          collapsedSize={0}
          minSize={PR_PANE.min}
          maxSize={PR_PANE.max}
          defaultSize={split ? prPaneWidth : 0}
          groupResizeBehavior="preserve-pixel-size"
          onResize={(size) => {
            pane.onResize(size);
            // Dragged shut while it is wanted: it goes back to its smallest width.
            if (split && size.inPixels === 0) queueMicrotask(() => pane.ref.current?.expand());
          }}
          className="border-l border-border"
        >
          {split ? <DetailPane /> : null}
        </Panel>
      </Group>
    </section>
  );
}
