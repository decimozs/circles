import type { UsageDay } from "@circles/bindings/UsageDay";

export type RangeId = "24h" | "7d" | "30d" | "90d";

export const RANGES: { id: RangeId; label: string; days: number }[] = [
  { id: "24h", label: "Past 24h", days: 1 },
  { id: "7d", label: "7 days", days: 7 },
  { id: "30d", label: "30 days", days: 30 },
  { id: "90d", label: "90 days", days: 90 },
];

const DAY_SECONDS = 86_400;

/** Unix seconds at the start of a range ending now. */
export function sinceFor(range: RangeId, now: number = Date.now()): number {
  const days = RANGES.find((r) => r.id === range)?.days ?? 30;
  return Math.floor(now / 1000) - days * DAY_SECONDS;
}

/** Dollars: `$461.90`, `$5,272.01`, and `<$0.01` for a little. */
export function money(value: number): string {
  if (value > 0 && value < 0.005) return "<$0.01";
  return value.toLocaleString("en-US", { style: "currency", currency: "USD" });
}

/** Token counts: `2.16B`, `4.63M`, `15.9K`, `812`. */
export function compact(count: number): string {
  const units: [number, string][] = [
    [1e9, "B"],
    [1e6, "M"],
    [1e3, "K"],
  ];
  for (const [size, suffix] of units) {
    if (count >= size) {
      const value = count / size;
      return `${value >= 100 ? value.toFixed(0) : value >= 10 ? value.toFixed(1) : value.toFixed(2)}${suffix}`;
    }
  }
  return String(count);
}

/** A share, `50.9%`, or `<0.1%` for a sliver. */
export function share(part: number, whole: number): string {
  if (whole <= 0 || part <= 0) return "0%";
  const percent = (part / whole) * 100;
  return percent < 0.1 ? "<0.1%" : `${percent.toFixed(1)}%`;
}

/** `2026-10-02` as `OCT 2`. */
export function dayLabel(day: string): string {
  const [, month, date] = day.split("-").map(Number);
  const names = [
    "JAN",
    "FEB",
    "MAR",
    "APR",
    "MAY",
    "JUN",
    "JUL",
    "AUG",
    "SEP",
    "OCT",
    "NOV",
    "DEC",
  ];
  return `${names[(month ?? 1) - 1]} ${date}`;
}

function isoDay(seconds: number): string {
  return new Date(seconds * 1000).toISOString().slice(0, 10);
}

/** One entry for every UTC day from `since` to `now`, with zero for days nothing was used. */
export function fillDays(days: UsageDay[], since: number, now: number = Date.now()): UsageDay[] {
  const byDay = new Map(days.map((d) => [d.day, d]));
  const out: UsageDay[] = [];
  const end = Math.floor(now / 1000);
  for (let at = since; isoDay(at) <= isoDay(end); at += DAY_SECONDS) {
    const day = isoDay(at);
    out.push(byDay.get(day) ?? { day, cost_usd: 0, tokens: 0 });
  }
  return out;
}

/** A round number at or above `value`, for the top of a chart's axis. */
export function niceCeiling(value: number): number {
  if (value <= 0) return 1;
  const power = 10 ** Math.floor(Math.log10(value));
  for (const step of [1, 2, 2.5, 5, 10]) {
    if (value <= step * power) return step * power;
  }
  return 10 * power;
}

/** A Claude folder as the person knows it: `~/.claude-work`. */
export function folderName(path: string): string {
  const match = path.match(/^\/(?:Users|home)\/[^/]+(\/.*)?$/);
  return match ? `~${match[1] ?? ""}` : path;
}
