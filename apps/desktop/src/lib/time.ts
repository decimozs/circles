const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** Compact age for lists: `now`, `5min`, `3h`, `2d`, then a short date. */
export function formatAge(timestamp: number, now: number = Date.now()): string {
  const diff = Math.max(0, now - timestamp);
  if (diff < MINUTE) return "now";
  if (diff < HOUR) return `${Math.floor(diff / MINUTE)}min`;
  if (diff < DAY) return `${Math.floor(diff / HOUR)}h`;
  if (diff < 7 * DAY) return `${Math.floor(diff / DAY)}d`;
  return new Date(timestamp).toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

export type RecencyGroup = "Today" | "Yesterday" | "Last 7 days" | "Older";

export const RECENCY_ORDER: RecencyGroup[] = ["Today", "Yesterday", "Last 7 days", "Older"];

/** Which Home group a timestamp belongs to, by local calendar day. */
export function recencyGroup(timestamp: number, now: number = Date.now()): RecencyGroup {
  const startOfToday = new Date(now);
  startOfToday.setHours(0, 0, 0, 0);
  const today = startOfToday.getTime();
  if (timestamp >= today) return "Today";
  const yesterday = new Date(today);
  yesterday.setDate(yesterday.getDate() - 1);
  if (timestamp >= yesterday.getTime()) return "Yesterday";
  const weekAgo = new Date(today);
  weekAgo.setDate(weekAgo.getDate() - 7);
  if (timestamp >= weekAgo.getTime()) return "Last 7 days";
  return "Older";
}

/** Groups items newest first, in the fixed group order, skipping empty groups. */
export function groupByRecency<T>(
  items: T[],
  timestampOf: (item: T) => number,
  now: number = Date.now(),
): { group: RecencyGroup; items: T[] }[] {
  const sorted = [...items].sort((a, b) => timestampOf(b) - timestampOf(a));
  return RECENCY_ORDER.map((group) => ({
    group,
    items: sorted.filter((item) => recencyGroup(timestampOf(item), now) === group),
  })).filter((g) => g.items.length > 0);
}
