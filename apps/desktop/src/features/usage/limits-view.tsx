import type { RateLimit } from "@circles/bindings/RateLimit";
import { useRateLimit } from "./api";

const WINDOWS: Record<string, string> = {
  five_hour: "5-hour window",
  seven_day: "7-day window",
  seven_day_opus: "7-day window, Opus",
  seven_day_sonnet: "7-day window, Sonnet",
  overage: "Extra usage",
};

function Meter({ label, value }: { label: string; value: number | null }) {
  const percent = value === null ? null : Math.round(Math.min(1, Math.max(0, value)) * 100);
  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-baseline justify-between text-sm">
        <span>{label}</span>
        <span className="tabular-nums text-muted-foreground">
          {percent === null ? "not reported" : `${percent}% used`}
        </span>
      </div>
      <meter className="sr-only" aria-label={label} min={0} max={100} value={percent ?? 0} />
      <div aria-hidden className="h-2 overflow-hidden rounded-full bg-muted">
        <div
          className={
            percent !== null && percent >= 90 ? "h-full bg-destructive" : "h-full bg-claude"
          }
          style={{ width: `${percent ?? 0}%` }}
        />
      </div>
    </div>
  );
}

function resets(limit: RateLimit): string | null {
  if (limit.resets_at === null) return null;
  return new Date(limit.resets_at * 1000).toLocaleString(undefined, {
    weekday: "short",
    hour: "numeric",
    minute: "2-digit",
  });
}

const STATUS: Record<string, string> = {
  allowed: "Within your limits",
  allowed_warning: "Close to a limit",
  rejected: "A limit has been reached",
};

/** The plan limits Claude Code last reported. It reports them while it works, so there is nothing
 * until a chat has run in this session of Circles. */
export function LimitsView({ enabled }: { enabled: boolean }) {
  const { data: limit, isPending } = useRateLimit(enabled);
  if (isPending) return <p className="px-6 py-6 text-sm text-muted-foreground">Loading…</p>;
  if (!limit) {
    return (
      <div className="flex flex-col gap-1 px-6 py-6 text-sm">
        <p>No limit information yet.</p>
        <p className="text-muted-foreground">
          Claude Code reports your plan's limits while it works. Send a message in any chat and they
          appear here.
        </p>
      </div>
    );
  }
  const until = resets(limit);
  return (
    <div className="flex max-w-xl flex-col gap-5 px-6 py-6">
      <p className="text-lg font-medium">{STATUS[limit.status] ?? limit.status}</p>
      <Meter label={WINDOWS.five_hour ?? "5-hour window"} value={limit.five_hour_utilization} />
      <Meter label={WINDOWS.seven_day ?? "7-day window"} value={limit.seven_day_utilization} />
      <ul className="flex flex-col gap-1 text-sm text-muted-foreground">
        <li>Last reported window: {WINDOWS[limit.window] ?? limit.window}</li>
        {until ? <li>Resets {until}</li> : null}
        {limit.using_overage ? <li>Extra usage is being used.</li> : null}
      </ul>
    </div>
  );
}
