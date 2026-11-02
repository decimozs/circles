import { cn } from "@amplify/ui/utils";
import { Loader2Icon, SparklesIcon } from "lucide-react";
import { useMemo } from "react";
import { Group, Panel, Separator } from "react-resizable-panels";
import { useProjects } from "@/features/projects/api";
import { usePanelSync } from "@/hooks/use-panel-sync";
import { useUserResizing } from "@/hooks/use-user-resizing";
import { PR_PANE, useUi } from "@/stores/ui";
import { useSkills } from "./api";
import { DetailPane } from "./detail-pane";
import { groupRows, matches, parseQuery, sortRows } from "./query";
import { SkillRow } from "./skill-row";
import { SkillsToolbar } from "./skills-toolbar";
import { useSkillsUi } from "./store";

/** The line between the list and the pane, as the sidebars have. */
const SEPARATOR =
  "w-px bg-border outline-none transition-colors data-[separator=hover]:bg-ring data-[separator=active]:bg-ring focus-visible:bg-ring";

/** The skills on this Mac and in the projects you have added, in one list. */
export function SkillsPage() {
  const projects = useProjects().data ?? [];
  const list = useSkills();
  const { scope, sort, filters, query, tabs, selected } = useSkillsUi();
  const { setScope, setSort, setFilters, setQuery, openTab } = useSkillsUi();
  // Something is open beside the list: the list narrows to a column.
  const split = tabs.length > 0;
  // The pane can be dragged wider or narrower, like the pull request pane, and shares its width.
  const paneWidth = useUi.getState().prPaneWidth;
  const setPaneWidth = useUi((s) => s.setPrPaneWidth);
  const resizing = useUserResizing();
  const pane = usePanelSync({
    visible: split,
    canShow: split,
    // Dragging it shut is not how it is closed: its tabs are. It comes back to its smallest width.
    setOpen: () => {},
    setWidth: setPaneWidth,
    remember: resizing.isResizing,
  });

  const all = list.data?.skills ?? [];
  const groups = useMemo(() => {
    const parsed = parseQuery(query);
    // A project chosen narrows the project skills only: yours, shared and plugins stay.
    const found = all.filter(
      (r) =>
        (scope === null || r.group !== "project" || r.project_id === scope) &&
        matches(r, parsed, filters),
    );
    return groupRows(sortRows(found, sort));
  }, [all, scope, query, filters, sort]);
  const shown = groups.reduce((n, g) => n + g.rows.length, 0);

  return (
    <section className="h-full" aria-label="Skills">
      <Group orientation="horizontal" className="h-full">
        <Panel id="skills-list" minSize={320}>
          <div className="h-full overflow-y-auto">
            <div
              className={cn("mx-auto flex flex-col gap-4 px-6 pt-4 pb-10", !split && "max-w-4xl")}
            >
              <h1 className="text-base font-semibold">Skills</h1>
              <SkillsToolbar
                query={query}
                onQuery={setQuery}
                sort={sort}
                onSort={setSort}
                filters={filters}
                onFilters={setFilters}
                projects={projects}
                scope={scope}
                onScope={setScope}
                fetching={list.isFetching}
                onRefresh={() => list.refetch()}
              />

              {list.isPending ? (
                <p
                  role="status"
                  className="flex items-center gap-2 py-6 text-sm text-muted-foreground"
                >
                  <Loader2Icon className="size-4 animate-spin" />
                  Looking for skills…
                </p>
              ) : list.isError ? (
                <p role="alert" className="py-6 text-sm text-destructive">
                  {String((list.error as { message?: string })?.message ?? list.error)}
                </p>
              ) : shown === 0 ? (
                <div className="flex flex-col items-center gap-1 py-16 text-center text-sm text-muted-foreground">
                  <SparklesIcon className="mb-2 size-6" />
                  <p>{all.length > 0 ? "No skill matches." : "No skills found."}</p>
                  {all.length === 0 ? (
                    <ul aria-label="Places looked in" className="mt-2 text-xs">
                      {(list.data?.roots ?? [])
                        .filter((r) => r.label !== r.path)
                        .map((r) => (
                          <li key={r.path} className="font-mono">
                            {r.path}
                          </li>
                        ))}
                    </ul>
                  ) : null}
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
                      <SkillRow
                        key={row.id}
                        row={row}
                        selected={selected === row.id}
                        onOpen={openTab}
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
          id="skills-pane"
          panelRef={pane.ref}
          collapsible
          collapsedSize={0}
          minSize={PR_PANE.min}
          maxSize={PR_PANE.max}
          defaultSize={split ? paneWidth : 0}
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
