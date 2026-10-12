/** The todos an agent suggests in a `circles-todos` block, which hold a JSON array of texts. */
export function parseAgentTodos(text: string): string[] {
  const found: string[] = [];
  for (const part of text.split("```circles-todos").slice(1)) {
    const body = part.split("```")[0] ?? "";
    try {
      const data: unknown = JSON.parse(body.trim());
      if (Array.isArray(data)) {
        for (const item of data) {
          if (typeof item === "string" && item.trim()) found.push(item.trim());
        }
      }
    } catch {
      // Not JSON: skipped.
    }
  }
  return found;
}

/** The context an `@todos` mention adds to a message: the open todos and how to add more. */
export function todosContext(todos: { text: string; done: boolean }[]): string {
  const open = todos.filter((t) => !t.done);
  const lines = open.length > 0 ? open.map((t) => `- [ ] ${t.text}`).join("\n") : "(none open)";
  return [
    "",
    "Todos for this workspace:",
    lines,
    "",
    "If you find follow-up work that should not be forgotten, end your reply with a fenced code block labelled circles-todos that holds a JSON array of short texts.",
  ].join("\n");
}
