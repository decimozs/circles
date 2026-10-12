import { describe, expect, it } from "vitest";
import { formatAge, groupByRecency, recencyGroup } from "@/lib/time";

// 8 October 2026, 15:30 local time.
const now = new Date(2026, 9, 8, 15, 30).getTime();
const at = (day: number, hour = 12) => new Date(2026, 9, day, hour).getTime();

describe("recencyGroup", () => {
  it("splits by local calendar day", () => {
    expect(recencyGroup(at(8, 0), now)).toBe("Today");
    expect(recencyGroup(now - 1000, now)).toBe("Today");
    expect(recencyGroup(at(7, 23), now)).toBe("Yesterday");
    expect(recencyGroup(at(7, 0), now)).toBe("Yesterday");
    expect(recencyGroup(at(6, 23), now)).toBe("Last 7 days");
    expect(recencyGroup(at(1, 0), now)).toBe("Last 7 days");
    expect(recencyGroup(at(1, 0) - 1, now)).toBe("Older");
    expect(recencyGroup(new Date(2026, 5, 1).getTime(), now)).toBe("Older");
  });
});

describe("groupByRecency", () => {
  it("orders groups, sorts newest first and skips empty groups", () => {
    const items = [
      { t: at(2) },
      { t: at(8, 9) },
      { t: at(7) },
      { t: at(8, 14) },
      { t: new Date(2026, 0, 1).getTime() },
    ];
    const groups = groupByRecency(items, (x) => x.t, now);
    expect(groups.map((g) => `${g.group}:${g.items.length}`)).toEqual([
      "Today:2",
      "Yesterday:1",
      "Last 7 days:1",
      "Older:1",
    ]);
    expect(groups[0].items[0].t).toBe(at(8, 14));
    expect(groupByRecency([], (x: { t: number }) => x.t, now)).toEqual([]);
  });
});

describe("formatAge", () => {
  it("is compact", () => {
    expect(formatAge(now - 10_000, now)).toBe("now");
    expect(formatAge(now - 5 * 60_000, now)).toBe("5min");
    expect(formatAge(now - 3 * 3_600_000, now)).toBe("3h");
    expect(formatAge(now - 2 * 86_400_000, now)).toBe("2d");
  });
});
