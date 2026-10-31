import type { UsageSummary } from "@circles/bindings/UsageSummary";
import { Breakdown, type Metric } from "./breakdown";
import { DailyChart } from "./daily-chart";
import { compact, fillDays, money, type RangeId, sinceFor } from "./format";
import { SplitBar } from "./split-bar";

/** The Cost and Tokens views: the same page, in dollars or in tokens. */
export function StatsView({
  metric,
  range,
  summary,
}: {
  metric: Metric;
  range: RangeId;
  summary: UsageSummary;
}) {
  const { totals } = summary;
  const days = fillDays(summary.by_day, sinceFor(range));
  const points = days.map((d) => ({
    day: d.day,
    value: metric === "cost" ? d.cost_usd : d.tokens,
  }));
  const unpriced = summary.by_model.filter((m) => !m.priced);

  return (
    <div className="flex flex-col gap-8 px-6 py-6">
      <div className="grid gap-8 md:grid-cols-[minmax(0,17rem)_minmax(0,1fr)]">
        <div className="flex flex-col gap-3">
          <p className="text-4xl font-semibold tabular-nums">
            {metric === "cost" ? money(totals.cost_usd) : compact(totals.processed)}
          </p>
          <p className="text-xs text-muted-foreground">
            {summary.sessions} {summary.sessions === 1 ? "session" : "sessions"} ·{" "}
            {metric === "cost" ? "API estimate" : "tokens processed"}
          </p>
          <div className="mt-2 flex flex-col gap-0.5 text-sm">
            <div className="flex items-center justify-between gap-3">
              <span className="flex items-center gap-2">
                <span className="size-2 rounded-full bg-claude" />
                Claude Code
                <span className="text-xs text-muted-foreground">
                  {summary.sessions} {summary.sessions === 1 ? "session" : "sessions"}
                </span>
              </span>
              <span className="tabular-nums">
                {metric === "cost" ? money(totals.cost_usd) : compact(totals.processed)}
              </span>
            </div>
            <p className="text-xs text-muted-foreground">
              {metric === "cost"
                ? `${compact(totals.processed)} tokens`
                : `${money(totals.cost_usd)} estimated`}
            </p>
          </div>
        </div>
        <DailyChart
          title={metric === "cost" ? "Daily cost" : "Daily tokens"}
          points={points}
          format={metric === "cost" ? money : compact}
        />
      </div>

      <section aria-label="Totals" className="flex flex-col gap-3">
        <h3 className="text-sm font-medium">Totals</h3>
        <dl className="grid grid-cols-2 gap-4 md:grid-cols-5">
          {[
            ["Processed tokens", compact(totals.processed)],
            ["Cached input", compact(totals.cached_input)],
            ["Uncached input", compact(totals.uncached_input)],
            ["Output", compact(totals.output)],
            ["Cache savings", money(totals.cache_savings_usd)],
          ].map(([label, value]) => (
            <div key={label} className="flex flex-col gap-0.5">
              <dt className="text-xs text-muted-foreground">{label}</dt>
              <dd className="text-lg tabular-nums">{value}</dd>
            </div>
          ))}
        </dl>
      </section>

      {metric === "cost" ? (
        <SplitBar
          title="Cost by type"
          segments={[
            {
              label: "Input",
              value: totals.cost_input_usd,
              shade: "var(--chart-3)",
              text: money(totals.cost_input_usd),
            },
            {
              label: "Cache read",
              value: totals.cost_cache_read_usd,
              shade: "var(--chart-2)",
              text: money(totals.cost_cache_read_usd),
            },
            {
              label: "Cache write",
              value: totals.cost_cache_write_usd,
              shade: "var(--chart-4)",
              text: money(totals.cost_cache_write_usd),
            },
            {
              label: "Output",
              value: totals.cost_output_usd,
              shade: "var(--chart-1)",
              text: money(totals.cost_output_usd),
            },
          ]}
        />
      ) : (
        <SplitBar
          title="Tokens by type"
          segments={[
            {
              label: "Cached input",
              value: totals.cached_input,
              shade: "var(--chart-3)",
              text: compact(totals.cached_input),
            },
            {
              label: "Uncached input",
              value: totals.uncached_input,
              shade: "var(--chart-4)",
              text: compact(totals.uncached_input),
            },
            {
              label: "Output",
              value: totals.output,
              shade: "var(--chart-1)",
              text: compact(totals.output),
            },
          ]}
        />
      )}

      <Breakdown metric={metric} models={summary.by_model} days={days} />

      <p className="text-xs text-muted-foreground">
        Cost is what these tokens would cost at the API's list prices. A subscription is not billed
        per token, so it is not what you pay.
        {unpriced.length > 0
          ? ` No price is known for ${unpriced.map((m) => m.model).join(", ")}: its tokens are counted, its cost is not.`
          : ""}
        {summary.skipped_lines > 0
          ? ` ${summary.skipped_lines} lines of Claude Code's history could not be read and were skipped.`
          : ""}
      </p>
    </div>
  );
}
