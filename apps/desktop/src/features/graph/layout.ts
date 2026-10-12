import type { GraphCommit } from "@circles/bindings/GraphCommit";

/** Number of colours the graph cycles through. See `--graph-1` to `--graph-5` in the tokens. */
export const COLORS = 5;

/** A stretch of line inside one row. Coordinates are lanes and fractions of the row height. */
export type Segment =
  | { kind: "line"; lane: number; from: "top" | "mid"; to: "mid" | "bottom"; color: number }
  | {
      kind: "curve";
      fromLane: number;
      toLane: number;
      /** `in`: from the top of the row to the dot. `out`: from the dot to the bottom. */
      half: "in" | "out";
      color: number;
    };

export type GraphRow = {
  commit: GraphCommit;
  lane: number;
  color: number;
  segments: Segment[];
};

export type GraphLayout = { rows: GraphRow[]; lanes: number };

type Lane = { sha: string; color: number };

/**
 * Places commits (children before parents) on lanes, like a railway map. A lane is a line that is
 * waiting for one commit. A new branch tip takes a free lane, a commit continues the lane that was
 * waiting for it, and a merge brings its other parents' lanes in.
 */
export function layoutGraph(commits: GraphCommit[]): GraphLayout {
  const lanes: (Lane | null)[] = [];
  let nextColor = 0;
  const rows: GraphRow[] = [];
  let width = 1;

  const free = () => {
    const at = lanes.indexOf(null);
    if (at >= 0) return at;
    lanes.push(null);
    return lanes.length - 1;
  };

  for (const commit of commits) {
    const before = lanes.map((l) => (l ? { ...l } : null));
    const waiting = lanes.flatMap((l, i) => (l?.sha === commit.sha ? [i] : []));
    let lane: number;
    let color: number;
    if (waiting.length > 0) {
      lane = waiting[0] as number;
      color = (lanes[lane] as Lane).color;
    } else {
      lane = free();
      color = nextColor++ % COLORS;
    }

    const segments: Segment[] = [];
    // Lines coming in from above: the lane itself, and any others that end here.
    if (waiting.length > 0) {
      for (const i of waiting) {
        const c = (before[i] as Lane).color;
        segments.push(
          i === lane
            ? { kind: "line", lane: i, from: "top", to: "mid", color: c }
            : { kind: "curve", fromLane: i, toLane: lane, half: "in", color: c },
        );
        lanes[i] = null;
      }
    }

    // Lines going down to the parents.
    commit.parents.forEach((parent, index) => {
      const held = lanes.findIndex((l) => l?.sha === parent);
      if (held >= 0 && index > 0) {
        // A merge from a lane that already exists.
        const c = (lanes[held] as Lane).color;
        segments.push({ kind: "curve", fromLane: lane, toLane: held, half: "out", color: c });
        return;
      }
      if (index === 0) {
        lanes[lane] = { sha: parent, color };
        segments.push({ kind: "line", lane, from: "mid", to: "bottom", color });
      } else {
        const target = free();
        const c = nextColor++ % COLORS;
        lanes[target] = { sha: parent, color: c };
        segments.push({ kind: "curve", fromLane: lane, toLane: target, half: "out", color: c });
      }
    });

    // Lines that pass straight through this row.
    for (let i = 0; i < before.length; i++) {
      const b = before[i];
      if (!b || waiting.includes(i) || i === lane) continue;
      segments.push({ kind: "line", lane: i, from: "top", to: "bottom", color: b.color });
    }

    while (lanes.length > 0 && lanes[lanes.length - 1] === null) lanes.pop();
    width = Math.max(width, lanes.length, before.length, lane + 1);
    rows.push({ commit, lane, color, segments });
  }
  return { rows, lanes: width };
}
