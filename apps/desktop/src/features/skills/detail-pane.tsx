import type { SkillDetail } from "@amplify/bindings/SkillDetail";
import { cn } from "@amplify/ui/utils";
import { FileTextIcon, Loader2Icon, SparklesIcon } from "lucide-react";
import { useState } from "react";
import { CopyButton, Markdown } from "@/features/chat/markdown";
import { formatAge } from "@/lib/time";
import { useSkillDetail, useSkillFile } from "./api";
import { SkillActions } from "./skill-actions";
import { type SkillTab, useSkillsUi } from "./store";

type View = "overview" | "text" | "files";

const VIEWS: { id: View; label: string }[] = [
  { id: "overview", label: "Overview" },
  { id: "text", label: "SKILL.md" },
  { id: "files", label: "Files" },
];

const GROUPS = {
  yours: "In one of your Claude folders",
  shared: "Shared with other agents, in ~/.agents",
  plugin: "From an installed plugin",
  project: "From a project, in its own folder",
} as const;

/** The text without the `---` block at its top, which the Overview shows as fields. */
export function withoutFrontMatter(text: string): string {
  const m = /^---\r?\n[\s\S]*?\r?\n---\r?\n?/.exec(text);
  return m ? text.slice(m[0].length) : text;
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-1">
      <dt className="text-xs text-muted-foreground">{label}</dt>
      <dd className="text-sm">{children}</dd>
    </div>
  );
}

function Overview({ detail }: { detail: SkillDetail }) {
  const { row, front } = detail;
  return (
    <dl className="flex flex-col gap-4 px-5 py-4">
      <Field label="What it is for">
        {row.description || <span className="text-muted-foreground italic">No description</span>}
      </Field>
      <Field label="Where it is">
        <span className="flex items-center gap-1">
          <code className="min-w-0 truncate rounded bg-muted px-1.5 py-0.5 font-mono text-xs">
            {row.path}
          </code>
          <CopyButton text={row.path} label="Copy path" />
        </span>
        <span className="mt-1 block text-xs text-muted-foreground">
          {GROUPS[row.group]}
          {row.group === "project" || row.group === "plugin" ? ` (${row.source})` : ""}.
          {row.also_in.length > 0 ? ` Also linked from ${row.also_in.join(", ")}.` : ""}
        </span>
      </Field>
      <Field label="The model may use it by itself">
        {front.disable_model_invocation ? "No, it runs only when you name it" : "Yes"}
      </Field>
      {front.argument_hint ? (
        <Field label="Arguments">
          <code className="font-mono text-xs">{front.argument_hint}</code>
        </Field>
      ) : null}
      {front.model ? <Field label="Model">{front.model}</Field> : null}
      {front.allowed_tools.length > 0 ? (
        <Field label="Tools it may use without asking">
          <ul className="flex flex-wrap gap-1">
            {front.allowed_tools.map((t) => (
              <li key={t} className="rounded bg-muted px-1.5 py-0.5 font-mono text-xs">
                {t}
              </li>
            ))}
          </ul>
        </Field>
      ) : null}
      <Field label="Files">
        {detail.files.length === 0
          ? "Only SKILL.md"
          : `${detail.files.length}${detail.truncated ? "+" : ""} besides SKILL.md`}
      </Field>
      {row.modified_ms > 0 ? <Field label="Changed">{formatAge(row.modified_ms)} ago</Field> : null}
    </dl>
  );
}

function Text({ detail }: { detail: SkillDetail }) {
  const [raw, setRaw] = useState(false);
  if (detail.skill_md === null) {
    return (
      <p className="px-5 py-4 text-sm text-muted-foreground">
        SKILL.md is too big to show here. Open it in your editor.
      </p>
    );
  }
  return (
    <div className="flex flex-col gap-3 px-5 py-4">
      <label className="flex items-center gap-2 self-end text-xs text-muted-foreground">
        <input type="checkbox" checked={raw} onChange={(e) => setRaw(e.target.checked)} />
        Show the raw text
      </label>
      {raw ? (
        <pre className="overflow-x-auto rounded-md bg-muted/50 p-3 font-mono text-xs leading-relaxed whitespace-pre-wrap">
          {detail.skill_md}
        </pre>
      ) : (
        // Rendered as Markdown with no raw HTML: a skill from a repository is not trusted.
        <Markdown text={withoutFrontMatter(detail.skill_md)} />
      )}
    </div>
  );
}

function FileText({ id, path }: { id: string; path: string }) {
  const file = useSkillFile(id, path);
  if (file.isPending) {
    return (
      <p role="status" className="flex items-center gap-2 text-xs text-muted-foreground">
        <Loader2Icon className="size-3 animate-spin" />
        Reading…
      </p>
    );
  }
  if (file.isError) {
    return (
      <p role="alert" className="text-xs text-muted-foreground">
        {String((file.error as { message?: string })?.message ?? file.error)}
      </p>
    );
  }
  return (
    <section aria-label={`Text of ${path}`}>
      <pre className="max-h-[60vh] overflow-auto rounded-md bg-muted/50 p-3 font-mono text-xs leading-relaxed whitespace-pre">
        {file.data}
      </pre>
    </section>
  );
}

function Files({ detail }: { detail: SkillDetail }) {
  const [open, setOpen] = useState<string | null>(null);
  if (detail.files.length === 0) {
    return <p className="px-5 py-4 text-sm text-muted-foreground">This skill is only SKILL.md.</p>;
  }
  return (
    <div className="flex flex-col gap-3 px-5 py-4">
      <ul aria-label="Files" className="flex flex-col">
        {detail.files.map((f) => (
          <li key={f.path}>
            <button
              type="button"
              aria-pressed={open === f.path}
              onClick={() => setOpen(open === f.path ? null : f.path)}
              className={cn(
                "flex w-full items-center gap-2 rounded-md px-2 py-1 text-left text-sm outline-none hover:bg-muted/50 focus-visible:ring-2 focus-visible:ring-ring/50",
                open === f.path && "bg-muted",
              )}
            >
              <FileTextIcon className="size-3.5 shrink-0 text-muted-foreground" />
              <span className="min-w-0 flex-1 truncate font-mono text-xs">{f.path}</span>
              <span className="shrink-0 text-xs text-muted-foreground tabular-nums">
                {f.size < 1024 ? `${f.size} B` : `${Math.round(f.size / 1024)} KB`}
              </span>
            </button>
          </li>
        ))}
      </ul>
      {detail.truncated ? (
        <p className="text-xs text-muted-foreground">More files than are listed here.</p>
      ) : null}
      {open ? <FileText id={detail.row.id} path={open} /> : null}
    </div>
  );
}

function Detail({ tab }: { tab: SkillTab }) {
  const [view, setView] = useState<View>("overview");
  const query = useSkillDetail(tab.key);
  if (query.isPending) {
    return (
      <p role="status" className="flex items-center gap-2 px-5 py-6 text-sm text-muted-foreground">
        <Loader2Icon className="size-4 animate-spin" />
        Loading {tab.name}…
      </p>
    );
  }
  if (query.isError) {
    return (
      <p role="alert" className="px-5 py-6 text-sm text-destructive">
        {String((query.error as { message?: string })?.message ?? query.error)}
      </p>
    );
  }
  const detail = query.data;
  const { row } = detail;
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <header className="flex flex-col gap-2 px-5 pt-4 pb-3">
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <span className="rounded-md bg-muted px-1.5 py-0.5 font-mono text-2xs">{row.source}</span>
          <span className="ml-auto" />
          <SkillActions row={row} />
        </div>
        <h2 className="flex items-start gap-2 text-lg leading-snug font-semibold">
          <SparklesIcon aria-hidden className="mt-1.5 size-4 shrink-0 text-muted-foreground" />
          {row.name}
        </h2>
        {row.group === "project" ? (
          <p className="text-xs text-muted-foreground">
            From a repository. Read it before you use it: it tells the agent what to do.
          </p>
        ) : null}
      </header>
      <div role="tablist" aria-label="Views" className="flex gap-1 border-b border-border px-4">
        {VIEWS.map((v) => (
          <button
            key={v.id}
            type="button"
            role="tab"
            aria-selected={view === v.id}
            onClick={() => setView(v.id)}
            className={cn(
              "-mb-px border-b-2 px-3 py-2 text-sm",
              view === v.id
                ? "border-foreground text-foreground"
                : "border-transparent text-muted-foreground hover:text-foreground",
            )}
          >
            {v.label}
          </button>
        ))}
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto">
        {view === "overview" ? <Overview detail={detail} /> : null}
        {view === "text" ? <Text detail={detail} /> : null}
        {view === "files" ? <Files detail={detail} /> : null}
      </div>
    </div>
  );
}

/** The right pane: the open skills as tabs, and the one that shows. */
export function DetailPane() {
  const { tabs, selected } = useSkillsUi();
  const tab = tabs.find((t) => t.key === selected);
  return (
    <div className="flex h-full min-w-0 flex-col">
      {tab ? (
        <Detail key={tab.key} tab={tab} />
      ) : (
        <p className="px-5 py-8 text-sm text-muted-foreground">
          Choose a skill in the list to open it here.
        </p>
      )}
    </div>
  );
}
