import type { GraphCommit } from "@circles/bindings/GraphCommit";
import { describe, expect, it } from "vitest";
import { layoutGraph } from "./layout";

const c = (sha: string, ...parents: string[]): GraphCommit => ({
  sha,
  parents,
  subject: sha,
  author: "a",
  time: 0,
  refs: [],
});

describe("layoutGraph", () => {
  it("keeps a straight history on one lane", () => {
    const { rows, lanes } = layoutGraph([c("c", "b"), c("b", "a"), c("a")]);
    expect(lanes).toBe(1);
    expect(rows.map((r) => r.lane)).toEqual([0, 0, 0]);
    expect(rows[0]?.segments).toContainEqual(
      expect.objectContaining({ kind: "line", from: "mid", to: "bottom" }),
    );
    expect(rows[2]?.segments.some((s) => s.kind === "line" && s.to === "bottom")).toBe(false);
  });

  it("puts a branch beside its base and joins them at the fork", () => {
    // feature (f2, f1) and main (m1) both grew from root.
    const { rows, lanes } = layoutGraph([
      c("f2", "f1"),
      c("m1", "root"),
      c("f1", "root"),
      c("root"),
    ]);
    expect(lanes).toBe(2);
    const lane = (sha: string) => rows.find((r) => r.commit.sha === sha)?.lane;
    expect(lane("f2")).toBe(0);
    expect(lane("f1")).toBe(0);
    expect(lane("m1")).toBe(1);
    // The two lines meet at the shared root, which sits on the first lane.
    expect(lane("root")).toBe(0);
    const root = rows[3];
    expect(root?.segments).toContainEqual(
      expect.objectContaining({ kind: "curve", fromLane: 1, toLane: 0, half: "in" }),
    );
  });

  it("draws a merge with a curve into the merged lane", () => {
    // merge M has parents a (first) and b (second); both come from root.
    const { rows, lanes } = layoutGraph([
      c("M", "a", "b"),
      c("a", "root"),
      c("b", "root"),
      c("root"),
    ]);
    expect(lanes).toBe(2);
    expect(rows[0]?.segments).toContainEqual(
      expect.objectContaining({ kind: "curve", fromLane: 0, toLane: 1, half: "out" }),
    );
    expect(rows.find((r) => r.commit.sha === "b")?.lane).toBe(1);
  });

  it("gives each new line its own colour", () => {
    const { rows } = layoutGraph([c("x", "r"), c("y", "r"), c("r")]);
    expect(rows[0]?.color).not.toBe(rows[1]?.color);
  });

  it("is empty for no commits", () => {
    expect(layoutGraph([])).toEqual({ rows: [], lanes: 1 });
  });
});
