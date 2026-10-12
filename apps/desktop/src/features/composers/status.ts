export type ComposerTone = "running" | "waiting" | "idle" | "new";

/** What a Composer is doing now, in a word. */
export function composerState(runs: { session: { status: string }; pending_approvals: number }[]): {
  label: string;
  tone: "running" | "waiting" | "idle" | "new";
} {
  if (runs.length === 0) return { label: "No runs yet", tone: "new" };
  if (runs.some((r) => r.session.status === "blocked" || r.pending_approvals > 0)) {
    return { label: "Waiting for you", tone: "waiting" };
  }
  if (runs.some((r) => r.session.status === "working")) {
    return { label: "Running", tone: "running" };
  }
  return { label: "Idle", tone: "idle" };
}
