import { Button } from "@circles/ui/components/button";
import { cn } from "@circles/ui/utils";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ExternalLinkIcon, Loader2Icon } from "lucide-react";
import { useState } from "react";
import { formatAge } from "@/lib/time";
import { usePrDetail } from "./api";
import { CodeTab } from "./code-tab";
import { Person } from "./person";
import { StateIcon } from "./pr-row";
import { type PrTab, usePrs } from "./store";
import { SummaryTab } from "./summary-tab";
import { TimelineTab } from "./timeline-tab";

type View = "summary" | "timeline" | "code";

const VIEWS: { id: View; label: string }[] = [
  { id: "summary", label: "Summary" },
  { id: "timeline", label: "Timeline" },
  { id: "code", label: "Code" },
];

function Detail({ tab }: { tab: PrTab }) {
  const [view, setView] = useState<View>("summary");
  const query = usePrDetail(tab.projectId, tab.number);

  if (query.isPending) {
    return (
      <p role="status" className="flex items-center gap-2 px-5 py-6 text-sm text-muted-foreground">
        <Loader2Icon className="size-4 animate-spin" />
        Loading #{tab.number}…
      </p>
    );
  }
  if (query.isError) {
    return (
      <p role="alert" className="px-5 py-6 text-sm text-destructive">
        {String((query.error as { message?: string })?.message ?? query.error)}
      </p>
    );
  }
  const detail = query.data;
  const { row } = detail;
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <header className="flex flex-col gap-2 px-5 pt-4 pb-3">
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <span className="min-w-0 truncate">{row.repo}</span>
          <span className="font-mono">#{row.number}</span>
          <Button
            variant="ghost"
            size="xs"
            className="ml-auto"
            onClick={() => openUrl(row.url).catch(() => {})}
          >
            <ExternalLinkIcon />
            Open on GitHub
          </Button>
        </div>
        <h2 className="flex items-start gap-2 text-lg leading-snug font-semibold">
          <StateIcon row={row} className="mt-1.5" />
          {row.title}
        </h2>
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
          <Person name={row.author} avatar={row.author_avatar} className="text-foreground" />
          <span>updated {formatAge(Date.parse(row.updated_at))} ago</span>
          {detail.refreshing || query.isFetching ? (
            <span role="status" className="flex items-center gap-1">
              <Loader2Icon className="size-3 animate-spin" />
              refreshing…
            </span>
          ) : null}
          <span className="font-mono">
            {row.base_ref} <span aria-hidden>←</span>
            <span className="sr-only">from</span> {row.head_ref}
          </span>
          <span>
            {row.changed_files} {row.changed_files === 1 ? "file" : "files"}{" "}
            <span className="text-success">+{row.additions.toLocaleString()}</span>{" "}
            <span className="text-destructive">-{row.deletions.toLocaleString()}</span>
          </span>
        </div>
      </header>
      <div role="tablist" aria-label="Views" className="flex gap-1 border-b border-border px-4">
        {VIEWS.map((v) => (
          <button
            key={v.id}
            type="button"
            role="tab"
            aria-selected={view === v.id}
            onClick={() => setView(v.id)}
            className={cn(
              "-mb-px border-b-2 px-3 py-2 text-sm",
              view === v.id
                ? "border-foreground text-foreground"
                : "border-transparent text-muted-foreground hover:text-foreground",
            )}
          >
            {v.label}
          </button>
        ))}
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto">
        {view === "summary" ? <SummaryTab detail={detail} /> : null}
        {view === "timeline" ? <TimelineTab detail={detail} /> : null}
        {view === "code" ? <CodeTab tab={tab} headSha={detail.head_sha} /> : null}
      </div>
    </div>
  );
}

/** The right pane: the open pull requests as tabs, and the one that shows. */
export function DetailPane() {
  const { tabs, selected } = usePrs();
  const tab = tabs.find((t) => t.key === selected);
  return (
    <div className="flex h-full min-w-0 flex-col">
      {tab ? (
        <Detail key={tab.key} tab={tab} />
      ) : (
        <p className="px-5 py-8 text-sm text-muted-foreground">
          Choose a pull request in the list to open it here.
        </p>
      )}
    </div>
  );
}
