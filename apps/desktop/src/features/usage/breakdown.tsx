import type { UsageDay } from "@circles/bindings/UsageDay";
import type { UsageModel } from "@circles/bindings/UsageModel";
import { cn } from "@circles/ui/utils";
import { useState } from "react";
import { compact, dayLabel, money, share } from "./format";

type By = "model" | "day";
export type Metric = "cost" | "tokens";

type Row = { key: string; name: string; cost: number; tokens: number; priced: boolean };

/** What was used, by model or by day, most first. */
export function Breakdown({
  metric,
  models,
  days,
}: {
  metric: Metric;
  models: UsageModel[];
  days: UsageDay[];
}) {
  const [by, setBy] = useState<By>("model");
  const value = (r: Row) => (metric === "cost" ? r.cost : r.tokens);
  const rows: Row[] =
    by === "model"
      ? models.map((m) => ({
          key: m.model,
          name: m.model,
          cost: m.cost_usd,
          tokens: m.tokens,
          priced: m.priced,
        }))
      : [...days]
          .reverse()
          .filter((d) => d.tokens > 0)
          .map((d) => ({
            key: d.day,
            name: dayLabel(d.day),
            cost: d.cost_usd,
            tokens: d.tokens,
            priced: true,
          }));
  const sorted = by === "model" ? [...rows].sort((a, b) => value(b) - value(a)) : rows;
  const whole = rows.reduce((sum, r) => sum + value(r), 0);
  const top = Math.max(0, ...rows.map(value));

  return (
    <section aria-label="Breakdown" className="flex flex-col gap-2">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-medium">Breakdown</h3>
        <fieldset className="flex rounded-md bg-muted p-0.5 text-xs">
          <legend className="sr-only">Group by</legend>
          {(["model", "day"] as const).map((option) => (
            <button
              key={option}
              type="button"
              aria-pressed={by === option}
              onClick={() => setBy(option)}
              className={cn(
                "rounded px-2.5 py-1 capitalize",
                by === option ? "bg-background text-foreground shadow-sm" : "text-muted-foreground",
              )}
            >
              {option}
            </button>
          ))}
        </fieldset>
      </div>
      {sorted.length === 0 ? (
        <p className="py-4 text-sm text-muted-foreground">Nothing used in this range.</p>
      ) : (
        <table className="w-full text-sm">
          <thead className="text-xs text-muted-foreground">
            <tr className="border-b border-border">
              <th className="w-8 py-1.5 text-left font-normal">#</th>
              <th className="py-1.5 text-left font-normal capitalize">{by}</th>
              <th className="py-1.5 text-right font-normal">
                {metric === "cost" ? "Cost" : "Tokens"}
              </th>
              <th className="w-20 py-1.5 text-right font-normal">Share</th>
              <th className="w-24 py-1.5 text-right font-normal">
                {metric === "cost" ? "Tokens" : "Cost"}
              </th>
            </tr>
          </thead>
          <tbody>
            {sorted.map((r, i) => (
              <tr key={r.key} className="border-b border-border/60 last:border-b-0">
                <td className="py-2 text-xs text-muted-foreground tabular-nums">{i + 1}</td>
                <td className="py-2">
                  <div className="font-mono text-xs">{r.name}</div>
                  <div
                    className="mt-1 h-px bg-claude"
                    style={{ width: `${top > 0 ? Math.max(2, (value(r) / top) * 60) : 0}%` }}
                  />
                </td>
                <td className="py-2 text-right tabular-nums">
                  {metric === "cost" ? (r.priced ? money(r.cost) : "no price") : compact(r.tokens)}
                </td>
                <td className="py-2 text-right tabular-nums text-muted-foreground">
                  {share(value(r), whole)}
                </td>
                <td className="py-2 text-right tabular-nums text-muted-foreground">
                  {metric === "cost" ? compact(r.tokens) : r.priced ? money(r.cost) : "no price"}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}
