import type { PrDetail } from "@circles/bindings/PrDetail";
import { cn } from "@circles/ui/utils";
import {
  CheckIcon,
  GitCommitHorizontalIcon,
  GitMergeIcon,
  GitPullRequestIcon,
  MessageSquareIcon,
  MessageSquareWarningIcon,
} from "lucide-react";
import { useMemo, useState } from "react";
import { Markdown } from "@/features/chat/markdown";
import { formatAge } from "@/lib/time";
import { Person } from "./person";
import { buildTimeline, type TimelineEvent } from "./timeline";

function Mark({ event }: { event: TimelineEvent }) {
  const cls = "size-4";
  if (event.kind === "opened") return <GitPullRequestIcon className={cn(cls, "text-success")} />;
  if (event.kind === "merged") return <GitMergeIcon className={cn(cls, "text-info")} />;
  if (event.kind === "commit") return <GitCommitHorizontalIcon className={cls} />;
  if (event.kind === "review" && event.detail === "approved") {
    return <CheckIcon className={cn(cls, "text-success")} />;
  }
  if (event.kind === "review" && event.detail === "changes_requested") {
    return <MessageSquareWarningIcon className={cn(cls, "text-warning")} />;
  }
  return <MessageSquareIcon className={cls} />;
}

/** Everything that happened to a pull request, in order. */
export function TimelineTab({ detail }: { detail: PrDetail }) {
  const [newestFirst, setNewestFirst] = useState(true);
  const events = useMemo(() => {
    const all = buildTimeline(detail);
    return newestFirst ? [...all].reverse() : all;
  }, [detail, newestFirst]);
  const commits = detail.commits.length;
  return (
    <div className="flex flex-col gap-3 px-5 py-4">
      <div className="flex items-center justify-between text-xs text-muted-foreground">
        <span>
          {commits} {commits === 1 ? "commit" : "commits"} ·{" "}
          {detail.comments.length + detail.reviews.length} comments and reviews
        </span>
        <button
          type="button"
          onClick={() => setNewestFirst((n) => !n)}
          className="hover:text-foreground"
        >
          {newestFirst ? "Newest first" : "Oldest first"}
        </button>
      </div>
      <ol aria-label="Timeline" className="flex flex-col">
        {events.map((e) => (
          <li key={e.id} className="flex gap-3 py-2">
            <span className="mt-0.5 shrink-0 text-muted-foreground">
              <Mark event={e} />
            </span>
            <div className="min-w-0 flex-1">
              <p className="flex flex-wrap items-baseline gap-x-2 text-sm">
                {e.kind === "commit" ? (
                  <>
                    <span className="min-w-0 truncate">{e.title}</span>
                    <code className="shrink-0 font-mono text-xs text-muted-foreground">
                      {e.detail}
                    </code>
                  </>
                ) : (
                  <>
                    <Person name={e.author} avatar={e.avatar} className="font-medium" />
                    <span>{e.title}</span>
                  </>
                )}
              </p>
              <p className="flex items-center gap-2 text-xs text-muted-foreground">
                {e.kind === "commit" ? <Person name={e.author} avatar={e.avatar} /> : null}
                <span>{Number.isNaN(e.at) ? "" : `${formatAge(e.at)} ago`}</span>
              </p>
              {e.body.trim() && e.kind !== "commit" ? (
                <div className="mt-1.5 rounded-lg border border-border p-3">
                  <Markdown text={e.body} />
                </div>
              ) : null}
            </div>
          </li>
        ))}
      </ol>
    </div>
  );
}
