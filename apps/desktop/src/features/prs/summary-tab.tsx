import type { PrDetail } from "@circles/bindings/PrDetail";
import { cn } from "@circles/ui/utils";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  CheckIcon,
  ChevronDownIcon,
  ChevronRightIcon,
  CircleCheckIcon,
  CircleDotDashedIcon,
  CircleXIcon,
  MessageSquareIcon,
  TagIcon,
  UsersIcon,
  XIcon,
} from "lucide-react";
import { useState } from "react";
import { Markdown } from "@/features/chat/markdown";
import { formatAge } from "@/lib/time";
import { Person } from "./person";

function Section({
  title,
  count,
  open,
  onToggle,
  children,
}: {
  title: string;
  count?: number;
  open: boolean;
  onToggle: () => void;
  children: React.ReactNode;
}) {
  return (
    <section aria-label={title} className="flex flex-col gap-2">
      <button
        type="button"
        aria-expanded={open}
        onClick={onToggle}
        className="flex w-fit items-center gap-1 text-sm text-muted-foreground hover:text-foreground"
      >
        {open ? (
          <ChevronDownIcon className="size-3.5" />
        ) : (
          <ChevronRightIcon className="size-3.5" />
        )}
        {title}
        {count !== undefined ? <span className="tabular-nums">({count})</span> : null}
      </button>
      {open ? children : null}
    </section>
  );
}

const CHECK_ICON = {
  passing: <CircleCheckIcon className="size-3.5 shrink-0 text-success" />,
  failing: <CircleXIcon className="size-3.5 shrink-0 text-destructive" />,
  pending: <CircleDotDashedIcon className="size-3.5 shrink-0 text-warning" />,
};

/** The reviews each person last left, newest word first. */
function lastReviews(detail: PrDetail) {
  const byPerson = new Map<string, PrDetail["reviews"][number]>();
  for (const review of detail.reviews) {
    if (review.state === "commented" && byPerson.has(review.author)) continue;
    byPerson.set(review.author, review);
  }
  return [...byPerson.values()];
}

/** The description, checks, who reviews, labels and comments of a pull request. */
export function SummaryTab({ detail }: { detail: PrDetail }) {
  const [descriptionOpen, setDescriptionOpen] = useState(true);
  const [checksOpen, setChecksOpen] = useState(false);
  const [commentsOpen, setCommentsOpen] = useState(true);
  const [newestFirst, setNewestFirst] = useState(true);
  const { row } = detail;
  const reviews = lastReviews(detail);
  const asked = row.requested_reviewers.filter((r) => !reviews.some((x) => x.author === r));
  const comments = newestFirst ? [...detail.comments].reverse() : detail.comments;
  const runs = detail.check_runs;
  const passing = runs.filter((c) => c.state === "passing").length;

  return (
    <div className="flex flex-col gap-5 px-5 py-4">
      <dl className="grid grid-cols-[auto_1fr] items-start gap-x-6 gap-y-2 text-sm">
        <dt className="flex items-center gap-1.5 text-muted-foreground">
          <UsersIcon className="size-3.5" />
          Reviewers
        </dt>
        <dd className="flex flex-wrap gap-x-4 gap-y-1">
          {reviews.length === 0 && asked.length === 0 ? (
            <span className="text-muted-foreground">None</span>
          ) : null}
          {reviews.map((r) => (
            <span key={r.author} className="flex items-center gap-1.5">
              <Person name={r.author} avatar={r.author_avatar} />
              {r.state === "approved" ? (
                <CheckIcon aria-label="Approved" className="size-3.5 text-success" />
              ) : r.state === "changes_requested" ? (
                <XIcon aria-label="Asked for changes" className="size-3.5 text-warning" />
              ) : (
                <MessageSquareIcon
                  aria-label="Commented"
                  className="size-3.5 text-muted-foreground"
                />
              )}
            </span>
          ))}
          {asked.map((name) => (
            <span key={name} className="flex items-center gap-1.5 text-muted-foreground">
              <Person name={name} avatar={`https://github.com/${name}.png?size=64`} />
              <span className="text-xs">asked</span>
            </span>
          ))}
        </dd>
        <dt className="flex items-center gap-1.5 text-muted-foreground">
          <TagIcon className="size-3.5" />
          Labels
        </dt>
        <dd className="flex flex-wrap gap-1.5">
          {row.labels.length === 0 ? <span className="text-muted-foreground">None</span> : null}
          {row.labels.map((l) => (
            <span
              key={l.name}
              className="rounded-full border border-border px-2 py-0.5 text-xs"
              style={l.color ? { borderColor: `#${l.color}` } : undefined}
            >
              {l.name}
            </span>
          ))}
        </dd>
      </dl>

      <Section
        title="Description"
        open={descriptionOpen}
        onToggle={() => setDescriptionOpen((o) => !o)}
      >
        {detail.body.trim() ? (
          <Markdown text={detail.body} />
        ) : (
          <p className="text-sm text-muted-foreground">No description.</p>
        )}
      </Section>

      <Section
        title={runs.length === 0 ? "Checks" : `Checks: ${passing} of ${runs.length} passing`}
        open={checksOpen}
        onToggle={() => setChecksOpen((o) => !o)}
      >
        {runs.length === 0 ? (
          <p className="text-sm text-muted-foreground">No checks ran.</p>
        ) : (
          <ul className="flex flex-col">
            {runs.map((c) => (
              <li key={`${c.workflow}/${c.name}`}>
                <button
                  type="button"
                  disabled={!c.url}
                  onClick={() => openUrl(c.url).catch(() => {})}
                  className="flex w-full items-center gap-2 rounded px-1 py-1 text-left text-sm hover:bg-muted/50 disabled:hover:bg-transparent"
                >
                  {CHECK_ICON[c.state as keyof typeof CHECK_ICON] ?? CHECK_ICON.pending}
                  <span className="min-w-0 truncate">{c.name}</span>
                  {c.workflow ? (
                    <span className="shrink-0 text-xs text-muted-foreground">{c.workflow}</span>
                  ) : null}
                </button>
              </li>
            ))}
          </ul>
        )}
      </Section>

      <Section
        title="Comments"
        count={detail.comments.length}
        open={commentsOpen}
        onToggle={() => setCommentsOpen((o) => !o)}
      >
        {detail.comments.length === 0 ? (
          <p className="text-sm text-muted-foreground">No comments yet.</p>
        ) : (
          <>
            <button
              type="button"
              onClick={() => setNewestFirst((n) => !n)}
              className="w-fit text-xs text-muted-foreground hover:text-foreground"
            >
              {newestFirst ? "Newest first" : "Oldest first"}
            </button>
            <ul className="flex flex-col gap-3">
              {comments.map((c) => (
                <li key={c.url || c.created_at} className="rounded-lg border border-border p-3">
                  <div className="mb-1.5 flex items-center gap-2 text-xs text-muted-foreground">
                    <Person name={c.author} avatar={c.author_avatar} className="text-foreground" />
                    <span>{formatAge(Date.parse(c.created_at))} ago</span>
                  </div>
                  <Markdown text={c.body} />
                </li>
              ))}
            </ul>
          </>
        )}
      </Section>
      <p
        className={cn(
          "text-xs text-muted-foreground",
          detail.mergeable === "conflicting" && "text-warning",
        )}
      >
        {detail.mergeable === "conflicting"
          ? "This branch has conflicts with its base."
          : row.state === "open"
            ? `Merge status: ${detail.merge_state.replace(/_/g, " ")}`
            : ""}
      </p>
    </div>
  );
}
