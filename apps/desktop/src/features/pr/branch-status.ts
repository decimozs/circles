import type { Readiness } from "@circles/bindings/Readiness";
import { stateOf, type Tone } from "./next-action";

/**
 * Where a workspace's branch stands, in one word and one colour: its pull request's state when it
 * has one, else whether it has commits or only changes that are not committed.
 */
export function branchStatus(
  readiness: Readiness | undefined,
): { label: string; tone: Tone; commitsOnly?: boolean } | null {
  if (!readiness) return null;
  if (readiness.pr) return stateOf(readiness.pr, readiness.ready);
  const { ahead, dirty } = readiness.git;
  if (ahead > 0) {
    return {
      label: ahead === 1 ? "1 commit" : `${ahead} commits`,
      tone: "violet",
      commitsOnly: true,
    };
  }
  if (dirty) return { label: "Uncommitted", tone: "muted" };
  return { label: "No changes", tone: "muted" };
}
