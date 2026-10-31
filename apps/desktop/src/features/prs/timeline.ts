import type { PrDetail } from "@circles/bindings/PrDetail";

export type TimelineEvent = {
  id: string;
  kind: "opened" | "commit" | "review" | "comment" | "merged";
  /** Milliseconds. */
  at: number;
  author: string;
  avatar: string;
  /** What happened, in a line. */
  title: string;
  body: string;
  /** For a commit, its short id. For a review, how it ended. */
  detail: string;
};

const REVIEW_TITLE: Record<string, string> = {
  approved: "approved these changes",
  changes_requested: "asked for changes",
  commented: "reviewed",
  dismissed: "had a review dismissed",
  pending: "started a review",
};

/** Everything that happened to a pull request, oldest first. A review that only holds inline
 * comments, with no text, is still an event: it says who looked. */
export function buildTimeline(detail: PrDetail): TimelineEvent[] {
  const events: TimelineEvent[] = [
    {
      id: "opened",
      kind: "opened",
      at: Date.parse(detail.row.created_at),
      author: detail.row.author,
      avatar: detail.row.author_avatar,
      title: "opened this pull request",
      body: "",
      detail: "",
    },
    ...detail.commits.map((c) => ({
      id: `commit-${c.sha}`,
      kind: "commit" as const,
      at: Date.parse(c.authored_at),
      author: c.author,
      avatar: c.author_avatar,
      title: c.headline,
      body: c.body,
      detail: c.sha.slice(0, 7),
    })),
    ...detail.reviews.map((r, i) => ({
      id: `review-${i}`,
      kind: "review" as const,
      at: Date.parse(r.submitted_at),
      author: r.author,
      avatar: r.author_avatar,
      title: REVIEW_TITLE[r.state] ?? "reviewed",
      body: r.body,
      detail: r.state,
    })),
    ...detail.comments.map((c, i) => ({
      id: `comment-${i}`,
      kind: "comment" as const,
      at: Date.parse(c.created_at),
      author: c.author,
      avatar: c.author_avatar,
      title: "commented",
      body: c.body,
      detail: "",
    })),
  ];
  if (detail.merged_at && detail.merged_by) {
    events.push({
      id: "merged",
      kind: "merged",
      at: Date.parse(detail.merged_at),
      author: detail.merged_by,
      avatar: `https://github.com/${detail.merged_by}.png?size=64`,
      title: "merged this pull request",
      body: "",
      detail: "",
    });
  }
  // A date that could not be read goes to the end, not the front.
  const when = (e: TimelineEvent) => (Number.isNaN(e.at) ? Number.MAX_SAFE_INTEGER : e.at);
  return events.sort((a, b) => when(a) - when(b));
}
