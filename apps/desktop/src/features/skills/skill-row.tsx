import type { SkillRow as Row } from "@amplify/bindings/SkillRow";
import { cn } from "@amplify/ui/utils";
import { SparklesIcon } from "lucide-react";
import { formatAge } from "@/lib/time";

/** One skill: its name, where it is from, what it says it is for, and how old it is. */
export function SkillRow({
  row,
  selected,
  onOpen,
}: {
  row: Row;
  selected?: boolean;
  onOpen: (row: Row) => void;
}) {
  const also = row.also_in.length > 0 ? `Also in ${row.also_in.join(", ")}` : undefined;
  return (
    <button
      type="button"
      aria-label={row.name}
      aria-current={selected ? "true" : undefined}
      title={also}
      onClick={() => onOpen(row)}
      className={cn(
        "flex w-full items-start gap-2.5 rounded-lg px-3 py-2 text-left outline-none hover:bg-muted/50 focus-visible:ring-2 focus-visible:ring-ring/50",
        selected && "bg-muted",
      )}
    >
      <SparklesIcon aria-hidden className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
      <span className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="flex min-w-0 items-center gap-2">
          <span className="min-w-0 truncate text-sm font-medium">{row.name}</span>
          <span className="shrink-0 rounded-md bg-muted px-1.5 py-0.5 font-mono text-2xs text-muted-foreground">
            {row.source}
          </span>
          {row.also_in.length > 0 ? (
            <span className="shrink-0 text-2xs text-muted-foreground">+{row.also_in.length}</span>
          ) : null}
          {row.model_invocable ? null : (
            <span className="shrink-0 rounded-md border border-border px-1.5 py-0.5 text-2xs text-muted-foreground">
              Manual only
            </span>
          )}
        </span>
        {row.description ? (
          <span className="line-clamp-2 text-xs text-muted-foreground">{row.description}</span>
        ) : (
          <span className="text-xs text-muted-foreground italic">No description</span>
        )}
      </span>
      <span className="shrink-0 pt-0.5 text-xs text-muted-foreground tabular-nums">
        {row.modified_ms > 0 ? formatAge(row.modified_ms) : ""}
      </span>
    </button>
  );
}
