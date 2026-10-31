import type { PrRow as Row } from "@circles/bindings/PrRow";
import { cn } from "@circles/ui/utils";
import {
  CircleCheckIcon,
  CircleDotDashedIcon,
  CircleXIcon,
  GitMergeIcon,
  GitPullRequestClosedIcon,
  GitPullRequestDraftIcon,
  GitPullRequestIcon,
  MessageSquareWarningIcon,
  UserCheckIcon,
} from "lucide-react";
import { formatAge } from "@/lib/time";

export function StateIcon({
  row,
  className,
}: {
  row: Pick<Row, "state" | "is_draft">;
  className?: string;
}) {
  const [Icon, tone, label] =
    row.state === "merged"
      ? [GitMergeIcon, "text-info", "Merged"]
      : row.state === "closed"
        ? [GitPullRequestClosedIcon, "text-destructive", "Closed"]
        : row.is_draft
          ? [GitPullRequestDraftIcon, "text-muted-foreground", "Draft"]
          : [GitPullRequestIcon, "text-success", "Open"];
  return <Icon aria-label={label} className={cn("size-4 shrink-0", tone, className)} />;
}

function Checks({ row }: { row: Row }) {
  if (row.checks.state === "none") return null;
  const label = `Checks ${row.checks.state}: ${row.checks.passed} of ${row.checks.total} passed`;
  if (row.checks.state === "passing") {
    return <CircleCheckIcon aria-label={label} className="size-3.5 shrink-0 text-success" />;
  }
  if (row.checks.state === "failing") {
    return <CircleXIcon aria-label={label} className="size-3.5 shrink-0 text-destructive" />;
  }
  return <CircleDotDashedIcon aria-label={label} className="size-3.5 shrink-0 text-warning" />;
}

/** One pull request: what it is, who made it, how big it is and how old. */
export function PrRow({
  row,
  selected,
  compact,
  onOpen,
  showProject,
}: {
  row: Row;
  selected?: boolean;
  compact?: boolean;
  onOpen: (row: Row) => void;
  showProject?: boolean;
}) {
  return (
    <button
      type="button"
      aria-label={`#${row.number} ${row.title}`}
      aria-current={selected ? "true" : undefined}
      onClick={() => onOpen(row)}
      className={cn(
        "flex w-full items-start gap-2.5 rounded-lg px-3 py-2 text-left outline-none hover:bg-muted/50 focus-visible:ring-2 focus-visible:ring-ring/50",
        selected && "bg-muted",
      )}
    >
      <span className="mt-0.5">
        <StateIcon row={row} />
      </span>
      <span className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="flex min-w-0 items-center gap-1.5 text-sm">
          <span className="shrink-0 font-mono text-xs text-muted-foreground">#{row.number}</span>
          <span className="min-w-0 truncate">{row.title}</span>
          <Checks row={row} />
          {row.review === "approved" ? (
            <UserCheckIcon aria-label="Approved" className="size-3.5 shrink-0 text-success" />
          ) : null}
          {row.review === "changes_requested" ? (
            <MessageSquareWarningIcon
              aria-label="Changes requested"
              className="size-3.5 shrink-0 text-warning"
            />
          ) : null}
        </span>
        <span className="flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground">
          {row.author ? (
            <img
              src={row.author_avatar}
              alt=""
              className="size-4 shrink-0 rounded-full bg-muted object-cover"
            />
          ) : null}
          <span className="shrink-0">{row.author || "unknown"}</span>
          <span className="min-w-0 truncate">{showProject ? row.repo : row.head_ref}</span>
        </span>
      </span>
      <span
        className={cn(
          "flex shrink-0 flex-col items-end gap-0.5 text-xs tabular-nums",
          compact && "min-w-14",
        )}
      >
        <span>
          <span className="text-success">+{row.additions.toLocaleString()}</span>{" "}
          <span className="text-destructive">-{row.deletions.toLocaleString()}</span>
        </span>
        <span className="text-muted-foreground">{formatAge(Date.parse(row.updated_at))} ago</span>
      </span>
    </button>
  );
}
