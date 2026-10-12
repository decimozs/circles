import type { NewComment } from "@circles/bindings/NewComment";

export type Finding = { path: string; line: number; endLine: number; body: string };

/** The label of the code block a review answers with. */
export const FINDINGS_FENCE = "circles-findings";

/** What the review agent is asked to do, and the shape it must answer in. */
export function reviewPrompt(baseBranch: string): string {
  return [
    `Review the changes in this workspace compared with ${baseBranch}. Use git diff to see them.`,
    "Look for bugs, risky or unclear code, missing error handling and missing tests. Do not change any files.",
    "",
    `When you are done, reply with a short summary, then one fenced code block labelled ${FINDINGS_FENCE} that contains a JSON array. Each finding is an object with "path" (relative to the workspace), "line" (the first line, counting from 1), optionally "end_line", and "body" (what is wrong and what to do).`,
    "Only include findings you are confident about. Use an empty array when there is nothing to report.",
  ].join("\n");
}

/** Findings from every `circles-findings` block in some text. Malformed ones are skipped. */
export function parseFindings(text: string): Finding[] {
  const found: Finding[] = [];
  const pattern = new RegExp(`\`\`\`${FINDINGS_FENCE}\\s*\\n([\\s\\S]*?)\`\`\``, "g");
  for (const match of text.matchAll(pattern)) {
    let data: unknown;
    try {
      data = JSON.parse(match[1]);
    } catch {
      continue;
    }
    if (!Array.isArray(data)) continue;
    for (const item of data as Record<string, unknown>[]) {
      const line = typeof item?.line === "number" ? Math.floor(item.line) : 0;
      const end = typeof item?.end_line === "number" ? Math.floor(item.end_line) : line;
      if (
        typeof item?.path !== "string" ||
        !item.path ||
        item.path.startsWith("/") ||
        item.path.split("/").includes("..") ||
        line < 1 ||
        typeof item.body !== "string" ||
        !item.body.trim()
      ) {
        continue;
      }
      found.push({ path: item.path, line, endLine: Math.max(end, line), body: item.body.trim() });
    }
  }
  return found;
}

export function asComment(finding: Finding): NewComment {
  return {
    path: finding.path,
    line_start: finding.line,
    line_end: finding.endLine,
    body: finding.body,
    source: "agent",
  };
}
