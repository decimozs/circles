import { cn } from "@circles/ui/utils";

export type Segment = { label: string; value: number; shade: string; text: string };

/** One bar cut into parts, with a legend. The shades come from the chart tokens. */
export function SplitBar({ title, segments }: { title: string; segments: Segment[] }) {
  const total = segments.reduce((sum, s) => sum + s.value, 0);
  return (
    <section aria-label={title} className="flex flex-col gap-2">
      <h3 className="text-sm font-medium">{title}</h3>
      <div className="flex h-2 w-full max-w-xl gap-px overflow-hidden rounded-full bg-muted">
        {segments.map((s) =>
          s.value > 0 && total > 0 ? (
            <span
              key={s.label}
              className="h-full"
              style={{ width: `${(s.value / total) * 100}%`, background: s.shade }}
            />
          ) : null,
        )}
      </div>
      <ul className="flex flex-wrap gap-x-4 gap-y-1 text-xs">
        {segments.map((s) => (
          <li key={s.label} className="flex items-center gap-1.5">
            <span className="size-2 rounded-sm" style={{ background: s.shade }} />
            <span className="text-muted-foreground">{s.label}</span>
            <span className={cn("font-medium tabular-nums")}>{s.text}</span>
          </li>
        ))}
      </ul>
    </section>
  );
}
