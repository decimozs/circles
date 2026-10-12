import type { Trigger } from "@circles/bindings/Trigger";
import type { TriggerInput } from "@circles/bindings/TriggerInput";
import { Button } from "@circles/ui/components/button";
import { Input } from "@circles/ui/components/input";
import { Switch } from "@circles/ui/components/switch";
import { Textarea } from "@circles/ui/components/textarea";
import { ClockIcon, PlusIcon, Trash2Icon } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { useJiraStatus } from "@/features/integrations/api";
import { useProjects } from "@/features/projects/api";
import { useDeleteTrigger, useSaveTrigger, useTriggers } from "./api";

const SELECT =
  "h-8 rounded-md border border-input bg-background px-2 text-xs outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50";

type Config = { every_minutes?: number; daily_at?: string; jql?: string };

/** One line that says when a trigger fires. */
export function describeTrigger(t: Pick<Trigger, "kind" | "config">): string {
  const config = (t.config ?? {}) as Config;
  if (t.kind === "schedule") {
    if (config.daily_at) return `Every day at ${config.daily_at}`;
    const n = config.every_minutes ?? 0;
    if (n % 1440 === 0 && n > 0) return n === 1440 ? "Every day" : `Every ${n / 1440} days`;
    if (n % 60 === 0 && n > 0) return n === 60 ? "Every hour" : `Every ${n / 60} hours`;
    return `Every ${n} minutes`;
  }
  if (t.kind === "pull_request") return "When a pull request opens or changes";
  if (t.kind === "check_failed") return "When checks fail on a pull request";
  if (t.kind === "jira_issue") return "When a Jira issue matches a query";
  return t.kind;
}

type Draft = {
  kind: "schedule" | "pull_request" | "check_failed" | "jira_issue";
  jql: string;
  mode: "every" | "daily";
  every: number;
  daily: string;
  projectId: string;
  prompt: string;
  enabled: boolean;
};

const blank = (): Draft => ({
  kind: "schedule",
  jql: "",
  mode: "daily",
  every: 60,
  daily: "09:00",
  projectId: "",
  prompt: "",
  enabled: true,
});

function toInput(d: Draft): TriggerInput {
  return {
    kind: d.kind,
    config:
      d.kind === "jira_issue"
        ? { jql: d.jql.trim() }
        : d.kind !== "schedule"
          ? {}
          : d.mode === "daily"
            ? { daily_at: d.daily }
            : { every_minutes: d.every },
    project_id: d.kind === "jira_issue" ? null : d.projectId || null,
    prompt: d.prompt,
    enabled: d.enabled,
  };
}

function fromTrigger(t: Trigger): Draft {
  const config = (t.config ?? {}) as Config;
  return {
    kind: t.kind as Draft["kind"],
    jql: config.jql ?? "",
    mode: config.daily_at ? "daily" : "every",
    every: config.every_minutes ?? 60,
    daily: config.daily_at ?? "09:00",
    projectId: t.project_id ?? "",
    prompt: t.prompt,
    enabled: t.enabled,
  };
}

function Form({
  composerId,
  trigger,
  onDone,
}: {
  composerId: string;
  trigger?: Trigger;
  onDone: () => void;
}) {
  const projects = useProjects().data ?? [];
  const save = useSaveTrigger();
  const [draft, setDraft] = useState<Draft>(() => (trigger ? fromTrigger(trigger) : blank()));
  const [error, setError] = useState<string | null>(null);
  const set = <K extends keyof Draft>(key: K, value: Draft[K]) =>
    setDraft((d) => ({ ...d, [key]: value }));
  const needsProject = draft.kind === "pull_request" || draft.kind === "check_failed";
  const jira = useJiraStatus().data;

  return (
    <div className="flex flex-col gap-2 rounded-md border border-border p-3 text-xs">
      <label className="flex flex-col gap-1">
        <span className="text-muted-foreground">Starts a run</span>
        <select
          aria-label="Trigger kind"
          className={SELECT}
          value={draft.kind}
          onChange={(e) => set("kind", e.target.value as Draft["kind"])}
        >
          <option value="schedule">On a schedule</option>
          <option value="pull_request">When a pull request opens or changes</option>
          <option value="check_failed">When checks fail on a pull request</option>
          <option value="jira_issue" disabled={!jira?.connected}>
            When a Jira issue matches a query{jira?.connected ? "" : " (connect Jira first)"}
          </option>
        </select>
      </label>
      {draft.kind === "jira_issue" ? (
        <div className="flex flex-col gap-1">
          <span className="text-muted-foreground">
            JQL query. Issues that match, new or changed, start a run. The Composer needs Jira
            switched on.
          </span>
          <Input
            aria-label="JQL query"
            value={draft.jql}
            onChange={(e) => set("jql", e.target.value)}
            placeholder="project = PROJ AND assignee = currentUser() AND statusCategory != Done"
            className="h-8 font-mono text-xs"
          />
        </div>
      ) : null}
      {draft.kind === "schedule" ? (
        <div className="flex items-end gap-2">
          <select
            aria-label="Schedule kind"
            className={SELECT}
            value={draft.mode}
            onChange={(e) => set("mode", e.target.value as Draft["mode"])}
          >
            <option value="daily">Every day at</option>
            <option value="every">Every N minutes</option>
          </select>
          {draft.mode === "daily" ? (
            <Input
              aria-label="Time of day"
              type="time"
              value={draft.daily}
              onChange={(e) => set("daily", e.target.value)}
              className="h-8 w-28 text-xs"
            />
          ) : (
            <Input
              aria-label="Minutes between runs"
              type="number"
              min={5}
              value={draft.every}
              onChange={(e) => set("every", Number(e.target.value))}
              className="h-8 w-24 text-xs"
            />
          )}
        </div>
      ) : null}
      {draft.kind === "jira_issue" ? null : (
        <label className="flex flex-col gap-1">
          <span className="text-muted-foreground">
            {needsProject ? "Project to watch" : "Project (optional, for context)"}
          </span>
          <select
            aria-label="Project"
            className={SELECT}
            value={draft.projectId}
            onChange={(e) => set("projectId", e.target.value)}
          >
            <option value="">{needsProject ? "Choose a project" : "None"}</option>
            {projects.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </label>
      )}
      <div className="flex flex-col gap-1">
        <span className="text-muted-foreground">What to tell it (the event is added below)</span>
        <Textarea
          aria-label="Trigger prompt"
          rows={3}
          value={draft.prompt}
          onChange={(e) => set("prompt", e.target.value)}
          className="text-xs"
        />
      </div>
      {error ? (
        <p role="alert" className="text-destructive">
          {error}
        </p>
      ) : null}
      <div className="flex justify-end gap-2">
        <Button size="sm" variant="outline" onClick={onDone}>
          Cancel
        </Button>
        <Button
          size="sm"
          disabled={
            save.isPending ||
            (needsProject && !draft.projectId) ||
            (draft.kind === "jira_issue" && !draft.jql.trim())
          }
          onClick={() =>
            save.mutate(
              { composerId, id: trigger?.id, input: toInput(draft) },
              {
                onSuccess: onDone,
                onError: (e) => setError((e as { message?: string }).message ?? String(e)),
              },
            )
          }
        >
          Save trigger
        </Button>
      </div>
    </div>
  );
}

/** When a Composer starts on its own. Runs started this way wait for you at anything that asks. */
export function TriggersEditor({ composerId }: { composerId: string }) {
  const triggers = useTriggers(composerId).data ?? [];
  const projects = useProjects().data ?? [];
  const save = useSaveTrigger();
  const remove = useDeleteTrigger();
  const [editing, setEditing] = useState<string | "new" | null>(null);

  return (
    <section aria-label="Triggers" className="flex flex-col gap-2">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-medium">Starts on its own</h3>
        <Button size="sm" variant="outline" onClick={() => setEditing("new")}>
          <PlusIcon />
          Add trigger
        </Button>
      </div>
      {triggers.length === 0 && editing !== "new" ? (
        <p className="text-xs text-muted-foreground">
          Nothing starts this Composer for you. Add a schedule or a trigger to run it while you are
          away. It only runs while Circles is open.
        </p>
      ) : null}
      {triggers.map((t) =>
        editing === t.id ? (
          <Form key={t.id} composerId={composerId} trigger={t} onDone={() => setEditing(null)} />
        ) : (
          <div
            key={t.id}
            className="flex items-center gap-2 rounded-md border border-border p-2 text-xs"
          >
            <ClockIcon className="size-3.5 shrink-0 text-muted-foreground" />
            <button
              type="button"
              className="min-w-0 flex-1 text-left"
              onClick={() => setEditing(t.id)}
            >
              <span className="block truncate">{describeTrigger(t)}</span>
              <span className="block truncate text-muted-foreground">
                {projects.find((p) => p.id === t.project_id)?.name ?? "Any project"}
                {!t.enabled && t.failures > 0 ? " · switched off after repeated failures" : ""}
              </span>
            </button>
            <Switch
              aria-label={`Trigger ${describeTrigger(t)} is on`}
              checked={t.enabled}
              onCheckedChange={(enabled) =>
                save.mutate(
                  {
                    composerId,
                    id: t.id,
                    input: {
                      kind: t.kind,
                      config: t.config,
                      project_id: t.project_id,
                      prompt: t.prompt,
                      enabled,
                    },
                  },
                  { onError: (e) => toast.error(String((e as { message?: string }).message ?? e)) },
                )
              }
            />
            <Button
              size="icon-sm"
              variant="ghost"
              aria-label={`Delete trigger ${describeTrigger(t)}`}
              onClick={() => remove.mutate(t.id)}
            >
              <Trash2Icon />
            </Button>
          </div>
        ),
      )}
      {editing === "new" ? <Form composerId={composerId} onDone={() => setEditing(null)} /> : null}
    </section>
  );
}
