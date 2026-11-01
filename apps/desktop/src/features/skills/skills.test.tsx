import type { SkillDetail } from "@circles/bindings/SkillDetail";
import type { SkillList } from "@circles/bindings/SkillList";
import type { SkillRow } from "@circles/bindings/SkillRow";
import { screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { backend } from "@/test/backend";
import { project } from "@/test/fixtures";
import { defaultBackend, renderApp } from "@/test/render";
import { groupRows, matches, NO_FILTERS, parseQuery, sortRows } from "./query";
import { useSkillsUi } from "./store";

function row(over: Partial<SkillRow> = {}): SkillRow {
  const name = over.name ?? "caveman";
  return {
    id: `/home/me/.claude/skills/${name}`,
    name,
    description: "Talk short.",
    group: "yours",
    source: "~/.claude",
    project_id: null,
    path: `/home/me/.claude/skills/${name}`,
    also_in: [],
    modified_ms: 1_000,
    files: 0,
    model_invocable: true,
    ...over,
  };
}

describe("searching and grouping", () => {
  it("understands words and key:value terms", () => {
    expect(parseQuery("short group:project project:circles source:~/.claude")).toEqual({
      words: ["short"],
      group: "project",
      project: "circles",
      source: "~/.claude",
    });
  });

  it("keeps only rows that satisfy every term and filter", () => {
    const a = row({ name: "caveman", description: "Talk short." });
    const b = row({
      name: "pr-title",
      description: "Titles of pull requests.",
      group: "project",
      source: "circles",
      project_id: "p1",
      files: 2,
      model_invocable: false,
    });
    const hits = (q: string, f = NO_FILTERS) =>
      [a, b].filter((r) => matches(r, parseQuery(q), f)).map((r) => r.name);
    expect(hits("")).toEqual(["caveman", "pr-title"]);
    expect(hits("titles")).toEqual(["pr-title"]);
    expect(hits("group:project")).toEqual(["pr-title"]);
    expect(hits("project:circles")).toEqual(["pr-title"]);
    expect(hits("project:nothing")).toEqual([]);
    expect(hits("", { ...NO_FILTERS, model: "no" })).toEqual(["pr-title"]);
    expect(hits("", { ...NO_FILTERS, files: true })).toEqual(["pr-title"]);
    expect(hits("", { ...NO_FILTERS, group: "yours" })).toEqual(["caveman"]);
  });

  it("sorts by name or by when it changed", () => {
    const rows = [
      row({ name: "b", modified_ms: 5 }),
      row({ name: "a", modified_ms: 1 }),
      row({ name: "c", modified_ms: 9 }),
    ];
    expect(sortRows(rows, "name").map((r) => r.name)).toEqual(["a", "b", "c"]);
    expect(sortRows(rows, "changed").map((r) => r.name)).toEqual(["c", "b", "a"]);
  });

  it("puts yours first, then shared, plugins and one section for each project", () => {
    const groups = groupRows([
      row({ name: "p2", group: "project", source: "zeta", project_id: "z" }),
      row({ name: "pl", group: "plugin", source: "gk" }),
      row({ name: "sh", group: "shared", source: "~/.agents" }),
      row({ name: "me" }),
      row({ name: "p1", group: "project", source: "alpha", project_id: "a" }),
    ]);
    expect(groups.map((g) => [g.title, g.rows.map((r) => r.name)])).toEqual([
      ["Yours", ["me"]],
      ["Shared", ["sh"]],
      ["Plugins", ["pl"]],
      ["alpha", ["p1"]],
      ["zeta", ["p2"]],
    ]);
    expect(groupRows([])).toEqual([]);
  });
});

function detail(r: SkillRow, over: Partial<SkillDetail> = {}): SkillDetail {
  return {
    row: r,
    front: {
      allowed_tools: ["Read", "Bash(ls:*)"],
      argument_hint: "[KEY-n]",
      model: null,
      disable_model_invocation: false,
    },
    skill_md: "---\nname: caveman\n---\n# Caveman\n\nTalk short.\n\n<b>raw html</b>\n",
    files: [{ path: "scripts/run.sh", size: 7 }],
    truncated: false,
    ...over,
  };
}

function seed(skills: SkillRow[], extra?: (d: SkillDetail | null) => void) {
  defaultBackend();
  const list: SkillList = {
    skills,
    roots: [{ label: "~/.claude", path: "~/.claude/skills", count: skills.length }],
  };
  backend
    .on("project_list", [project()])
    .on("skills_list", list)
    .on("skills_detail", skills[0] ? detail(skills[0]) : null);
  extra?.(null);
}

describe("the Skills page", () => {
  beforeEach(() => {
    localStorage.clear();
    useSkillsUi.setState({
      tabs: [],
      selected: null,
      scope: null,
      sort: "name",
      filters: NO_FILTERS,
      query: "",
    });
  });

  it("lists skills in groups with counts, and notes the ones linked from elsewhere", async () => {
    seed([
      row({ name: "caveman", also_in: ["~/.claude-work"] }),
      row({ name: "herdr", group: "shared", source: "~/.agents" }),
      row({
        name: "pr-title",
        group: "project",
        source: "circles",
        project_id: "p1",
        model_invocable: false,
      }),
    ]);
    renderApp("/skills");
    expect(await screen.findByRole("button", { name: "caveman" })).toHaveAttribute(
      "title",
      "Also in ~/.claude-work",
    );
    for (const group of ["Yours", "Shared", "circles"]) {
      expect(screen.getByRole("region", { name: group })).toBeInTheDocument();
    }
    expect(screen.getByText("Manual only")).toBeInTheDocument();
    // Skills is the item before Pull requests.
    const nav = screen.getByRole("navigation", { name: "Primary" });
    const names = [...nav.querySelectorAll("a, button")].map((e) => e.textContent?.trim());
    expect(names.slice(-2)).toEqual(["Skills", "Pull requests"]);
  });

  it("narrows the list as you type, and says when nothing matches", async () => {
    seed([row({ name: "caveman" }), row({ name: "herdr", description: "Terminals." })]);
    const { user } = renderApp("/skills");
    await screen.findByRole("button", { name: "caveman" });
    await user.type(screen.getByLabelText("Search skills"), "terminals");
    expect(screen.queryByRole("button", { name: "caveman" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "herdr" })).toBeInTheDocument();
    await user.clear(screen.getByLabelText("Search skills"));
    await user.type(screen.getByLabelText("Search skills"), "zzz");
    expect(await screen.findByText("No skill matches.")).toBeInTheDocument();
  });

  it("says where it looked when there are no skills", async () => {
    seed([]);
    renderApp("/skills");
    expect(await screen.findByText("No skills found.")).toBeInTheDocument();
    expect(screen.getByRole("list", { name: "Places looked in" })).toHaveTextContent(
      "~/.claude/skills",
    );
  });

  it("opens a skill beside the list, as a tab, with what it says and its files", async () => {
    seed([row({ name: "caveman", files: 1 })]);
    const { user } = renderApp("/skills");
    await user.click(await screen.findByRole("button", { name: "caveman" }));
    const tab = await screen.findByRole("tab", { name: "caveman" });
    expect(tab).toHaveAttribute("aria-selected", "true");
    // Overview: description, tools and where it is.
    expect(await screen.findByText("Read")).toBeInTheDocument();
    expect(screen.getByText("Bash(ls:*)")).toBeInTheDocument();
    expect(screen.getByText("[KEY-n]")).toBeInTheDocument();
    expect(screen.getByText("/home/me/.claude/skills/caveman")).toBeInTheDocument();
    // SKILL.md: rendered with no front matter, and raw HTML shown as text, not run.
    await user.click(screen.getByRole("tab", { name: "SKILL.md" }));
    expect(await screen.findByRole("heading", { name: "Caveman" })).toBeInTheDocument();
    expect(screen.queryByText("name: caveman")).not.toBeInTheDocument();
    expect(document.querySelector("b")).toBeNull();
    await user.click(screen.getByLabelText("Show the raw text"));
    expect(screen.getByText(/name: caveman/)).toBeInTheDocument();
    // Files: one opens as text.
    backend.on("skills_file", "echo hi\n");
    await user.click(screen.getByRole("tab", { name: "Files" }));
    await user.click(await screen.findByRole("button", { name: /scripts\/run\.sh/ }));
    expect(await screen.findByRole("region", { name: "Text of scripts/run.sh" })).toHaveTextContent(
      "echo hi",
    );
    expect(backend.callsTo("skills_file").slice(-1)[0]).toEqual({
      id: "/home/me/.claude/skills/caveman",
      path: "scripts/run.sh",
    });
    // Closing the tab goes back to the list alone.
    await user.click(screen.getByRole("button", { name: "Close caveman" }));
    await waitFor(() => expect(screen.queryByRole("tab", { name: "caveman" })).toBeNull());
  });

  it("warns that a project's skill comes from a repository", async () => {
    const r = row({ name: "pr-title", group: "project", source: "circles", project_id: "p1" });
    seed([r]);
    const { user } = renderApp("/skills");
    await user.click(await screen.findByRole("button", { name: "pr-title" }));
    const pane = await screen.findByRole("tablist", { name: "Views" });
    expect(within(pane.parentElement as HTMLElement).getByText(/From a repository/)).toBeVisible();
  });

  it("shows a project's skills only for the project chosen, and keeps the others", async () => {
    seed([
      row({ name: "mine" }),
      row({ name: "a-skill", group: "project", source: "alpha", project_id: "a" }),
      row({ name: "b-skill", group: "project", source: "beta", project_id: "b" }),
    ]);
    useSkillsUi.setState({ scope: "a" });
    renderApp("/skills");
    await screen.findByRole("button", { name: "mine" });
    expect(screen.getByRole("button", { name: "a-skill" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "b-skill" })).not.toBeInTheDocument();
  });
});
