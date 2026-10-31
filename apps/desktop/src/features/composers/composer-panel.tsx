import type { AuditEntry } from "@circles/bindings/AuditEntry";
import type { Autonomy } from "@circles/bindings/Autonomy";
import type { Composer } from "@circles/bindings/Composer";
import type { ComposerInput } from "@circles/bindings/ComposerInput";
import type { FleetItem } from "@circles/bindings/FleetItem";
import type { Limits } from "@circles/bindings/Limits";
import type { Tier } from "@circles/bindings/Tier";
import { Badge } from "@circles/ui/components/badge";
import { Button } from "@circles/ui/components/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@circles/ui/components/dialog";
import { Input } from "@circles/ui/components/input";
import { Textarea } from "@circles/ui/components/textarea";
import { cn } from "@circles/ui/utils";
import { Loader2Icon, Maximize2Icon, SettingsIcon, Trash2Icon } from "lucide-react";
import { useEffect, useState } from "react";
import { useNavigate } from "react-router";
import { toast } from "sonner";
import { JiraIcon } from "@/components/jira-icon";
import { useChanges } from "@/features/files/api";
import { useJiraStatus } from "@/features/integrations/api";
import { useReadiness } from "@/features/pr/api";
import { branchStatus } from "@/features/pr/branch-status";
import { TONE } from "@/features/pr/next-action";
import { useProjects } from "@/features/projects/api";
import { ProjectAvatar } from "@/features/projects/project-avatar";
import { formatAge } from "@/lib/time";
import {
  autonomyOf,
  isRisky,
  MODES,
  modeAutonomy,
  TIER_LABEL,
  useApprovals,
  useAudit,
  useDeleteComposer,
  useFleet,
  useStopChild,
  useToolCatalog,
  useUpdateComposer,
} from "./api";
import { ApprovalCard, toolTitle } from "./approval-card";
import {
  InstructionsField,
  IntegrationsField,
  NameModeFields,
  RepositoriesField,
} from "./composer-fields";
import { FleetGraph } from "./fleet-graph";
import { TriggersEditor } from "./triggers-editor";

export type Tab = "fleet" | "approvals" | "steps" | "details";

const TABS: { id: Tab; label: string }[] = [
  { id: "fleet", label: "Fleet" },
  { id: "approvals", label: "Approvals" },
  { id: "steps", label: "Steps" },
  { id: "details", label: "Details" },
];

function Empty({ children }: { children: React.ReactNode }) {
  return <p className="px-4 py-6 text-center text-xs text-muted-foreground">{children}</p>;
}

export type FleetGroup = {
  key: string;
  workspace: FleetItem["workspace"];
  chats: NonNullable<FleetItem["chat"]>[];
};

/**
 * One group per workspace. A run records starting a workspace and starting an agent in it as two
 * separate things, so the same workspace would otherwise be listed twice.
 */
export function groupFleet(items: FleetItem[]): FleetGroup[] {
  const groups = new Map<string, FleetGroup>();
  for (const item of items) {
    const key = item.workspace?.id ?? item.child.workspace_id ?? item.child.id;
    const group = groups.get(key) ?? { key, workspace: item.workspace, chats: [] };
    group.workspace = group.workspace ?? item.workspace;
    const chat = item.chat;
    if (chat && !group.chats.some((c) => c.id === chat.id)) group.chats.push(chat);
    groups.set(key, group);
  }
  return [...groups.values()];
}

export type FleetTone = "working" | "waiting" | "idle";

const FLEET_DOT: Record<FleetTone, string> = {
  working: "bg-success animate-pulse",
  waiting: "bg-warning animate-pulse",
  idle: "bg-muted-foreground/40",
};

/** What a workspace is doing, from its agents: waiting on you first, then working, else idle. */
export function fleetTone(group: FleetGroup): FleetTone {
  if (group.chats.some((c) => c.status === "blocked")) return "waiting";
  if (group.chats.some((c) => c.status === "working")) return "working";
  return "idle";
}

/** One workspace of the fleet: a card that opens it. Its state, branch and where the branch stands. */
export function FleetCard({
  group,
  tone,
  name,
}: {
  group: FleetGroup;
  tone: FleetTone;
  name: string;
}) {
  const navigate = useNavigate();
  const stop = useStopChild();
  const ws = group.workspace;
  const readiness = useReadiness(ws?.id, !!ws);
  const status = branchStatus(readiness.data);
  const colors = status ? TONE[status.tone] : null;
  // A branch with commits and no pull request shows what it changed, not a badge.
  const commitsOnly = !!status?.commitsOnly;
  const changes = useChanges(commitsOnly ? ws?.id : undefined, "all").data ?? [];
  const added = changes.reduce((n, f) => n + f.added, 0);
  const removed = changes.reduce((n, f) => n + f.removed, 0);
  // Only an agent that is going has anything to do here: stopping it.
  const active = group.chats.filter((c) => c.status === "working" || c.status === "blocked");
  const chip = "flex h-5 shrink-0 items-center rounded-md px-1.5 text-2xs font-medium";
  return (
    <div
      data-testid="fleet-card"
      className={cn(
        "relative rounded-md border",
        colors?.row ?? "border-border",
        tone === "waiting" && "border-warning/50",
        ws && "hover:brightness-110",
      )}
    >
      {/* The whole card is the way in. The text above it does not take the click. */}
      {ws ? (
        <button
          type="button"
          aria-label={`Open ${name}`}
          onClick={() => navigate(`/workspace/${ws.id}`)}
          className="absolute inset-0 rounded-md outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
        />
      ) : null}
      <div className="pointer-events-none relative flex items-start gap-2 px-2 py-1.5">
        <span
          role="img"
          aria-label={tone === "waiting" ? "Waiting" : tone === "working" ? "Working" : "Idle"}
          className={cn("mt-2 size-2 shrink-0 rounded-full", FLEET_DOT[tone])}
        />
        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-medium">{name}</p>
          <p className="truncate text-muted-foreground">{ws?.branch ?? ""}</p>
          {ws?.pr_number || (status && !commitsOnly) ? (
            <p className="mt-1 flex items-center gap-1">
              {ws?.pr_number ? (
                <span className={cn(chip, colors?.chip ?? "bg-muted text-muted-foreground")}>
                  #{ws.pr_number}
                </span>
              ) : null}
              {status && !commitsOnly ? (
                <span data-testid="branch-status" className={cn(chip, colors?.badge)}>
                  {status.label}
                </span>
              ) : null}
            </p>
          ) : null}
          {commitsOnly && changes.length > 0 ? (
            <p
              data-testid="branch-changes"
              className="mt-1 flex items-center gap-2 text-muted-foreground"
            >
              <span>
                {changes.length} {changes.length === 1 ? "file" : "files"}
              </span>
              <span className="flex gap-1.5">
                <span className="font-mono text-success">+{added}</span>
                <span className="font-mono text-destructive">−{removed}</span>
              </span>
            </p>
          ) : null}
        </div>
        {active.length > 0 ? (
          <Button
            size="sm"
            variant="outline"
            className="pointer-events-auto"
            disabled={stop.isPending}
            onClick={() => {
              for (const chat of active) {
                stop.mutate(chat.id, {
                  onError: (e) => toast.error(e instanceof Error ? e.message : String(e)),
                });
              }
            }}
          >
            Stop
          </Button>
        ) : null}
      </div>
    </div>
  );
}

/** The workspaces and agents this run started, as a tree under the Composer. */
function FleetTab({ composer, runId }: { composer: Composer; runId?: string }) {
  const fleet = useFleet(composer.id, runId);
  const navigate = useNavigate();
  const [graph, setGraph] = useState(false);
  const groups = groupFleet(fleet.data ?? []);
  if (groups.length === 0) {
    return <Empty>This run has not started any workspace or agent yet.</Empty>;
  }
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto p-3 text-xs">
        <p className="flex items-center gap-2 font-medium">
          <span aria-hidden className="size-2 rounded-full bg-foreground" />
          <span className="min-w-0 flex-1 truncate">{composer.name}</span>
          <span className="font-normal text-muted-foreground">
            {groups.length} {groups.length === 1 ? "workspace" : "workspaces"}
          </span>
        </p>
        <ul aria-label="Fleet" className="ml-[3.5px] flex flex-col pt-2">
          {groups.map((group, index) => {
            const first = index === 0;
            const last = index === groups.length - 1;
            const ws = group.workspace;
            const tone = fleetTone(group);
            const name = ws?.title ?? ws?.city ?? "A removed workspace";
            return (
              <li key={group.key} className={cn("relative flex flex-col pl-4", !last && "pb-2")}>
                {/* The line runs from the circle above to this card, and on to the next one. */}
                <span
                  aria-hidden
                  className={cn(
                    "absolute left-0 w-px bg-border",
                    first && last && "-top-2 h-6",
                    first && !last && "-top-2 h-[calc(100%+0.5rem)]",
                    !first && last && "top-0 h-4",
                    !first && !last && "top-0 h-full",
                  )}
                />
                <span aria-hidden className="absolute top-4 left-0 h-px w-4 bg-border" />
                <FleetCard group={group} tone={tone} name={name} />
              </li>
            );
          })}
        </ul>
      </div>
      <div className="shrink-0 border-t border-border p-3">
        <Button
          size="sm"
          variant="outline"
          aria-label="Expand fleet"
          onClick={() => setGraph(true)}
        >
          <Maximize2Icon />
          Expand
        </Button>
      </div>
      <Dialog open={graph} onOpenChange={setGraph}>
        <DialogContent className="flex h-[80vh] flex-col gap-0 p-0 sm:max-w-5xl">
          <DialogHeader className="shrink-0 border-b border-border px-5 py-3">
            <DialogTitle>{composer.name} fleet</DialogTitle>
            <DialogDescription className="sr-only">
              The workspaces and agents this run started, drawn as a graph.
            </DialogDescription>
          </DialogHeader>
          <div className="min-h-0 flex-1 overflow-auto">
            <FleetGraph
              name={composer.name}
              groups={groups}
              onOpen={(id) => {
                setGraph(false);
                navigate(`/workspace/${id}`);
              }}
            />
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}

function ApprovalsTab({ composer }: { composer: Composer }) {
  const all = useApprovals({ composerId: composer.id }).data ?? [];
  const pending = all.filter((a) => a.status === "pending");
  const done = all.filter((a) => a.status !== "pending").slice(0, 20);
  if (all.length === 0) return <Empty>Nothing has asked for approval yet.</Empty>;
  return (
    <div className="flex flex-col gap-3 p-3">
      {pending.map((a) => (
        <ApprovalCard key={a.id} approval={a} />
      ))}
      {pending.length === 0 ? (
        <p className="text-xs text-muted-foreground">Nothing is waiting for you.</p>
      ) : null}
      {done.length > 0 ? (
        <ul aria-label="Answered" className="flex flex-col gap-1 text-xs">
          {done.map((a) => (
            <li key={a.id} className="flex items-center justify-between gap-2">
              <span className="truncate">{toolTitle(a.tool)}</span>
              <span
                className={cn(
                  "shrink-0 rounded px-1.5 py-0.5",
                  a.status === "allowed"
                    ? "bg-success/15 text-success"
                    : "bg-muted text-muted-foreground",
                )}
              >
                {a.status}
              </span>
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}

const OUTCOME: Record<string, string> = {
  ok: "bg-success/15 text-success",
  error: "bg-destructive/15 text-destructive",
  denied: "bg-warning/15 text-warning",
  expired: "bg-muted text-muted-foreground",
};

/** One call, as it appears in the audit lists. */
export function AuditRow({ entry }: { entry: AuditEntry }) {
  return (
    <li className="flex flex-col gap-0.5 px-3 py-2 text-xs">
      <div className="flex items-center gap-2">
        <span className="font-medium">{toolTitle(entry.tool)}</span>
        <span className={cn("rounded px-1.5 py-0.5", OUTCOME[entry.outcome] ?? "bg-muted")}>
          {entry.outcome}
        </span>
        {entry.tier ? <span className="text-muted-foreground">{entry.tier}</span> : null}
        <span className="ml-auto text-muted-foreground">{formatAge(entry.at)}</span>
      </div>
      {entry.summary ? (
        <p className="line-clamp-2 break-words text-muted-foreground" title={entry.summary}>
          {entry.summary}
        </p>
      ) : null}
    </li>
  );
}

/** Every tool call of this run, newest first. */
function StepsTab({ composer, runId }: { composer: Composer; runId?: string }) {
  const entries = useAudit({ composerId: composer.id, sessionId: runId }).data ?? [];
  if (entries.length === 0) return <Empty>No tool has been called yet.</Empty>;
  return (
    <ul aria-label="Steps" className="flex flex-col divide-y divide-border">
      {entries.map((e) => (
        <AuditRow key={e.id} entry={e} />
      ))}
    </ul>
  );
}

function Notes({ value, onChange }: { value: string; onChange: (notes: string) => void }) {
  return (
    <section aria-label="Notes" className="flex flex-col gap-1.5">
      <h3 className="text-sm font-medium">Notes</h3>
      <p className="text-xs text-muted-foreground">
        Kept between runs and read by the Composer at the start of each. Write what it should
        remember about your projects. Up to 4000 characters.
      </p>
      <Textarea
        aria-label="Notes"
        rows={8}
        value={value}
        maxLength={4000}
        onChange={(e) => onChange(e.target.value)}
        className="text-xs"
      />
    </section>
  );
}

/** The repositories a Composer may work on: the ones picked, or all of them. */
function RepositoriesSection({ composer }: { composer: Composer }) {
  const projects = useProjects().data ?? [];
  const picked = composer.projects;
  const all = picked === null;
  const handled = picked
    ? picked
        .map((id) => projects.find((p) => p.id === id))
        .filter((p): p is NonNullable<typeof p> => !!p)
    : projects;
  const gone = picked ? picked.length - handled.length : 0;
  const LIMIT = 8;
  const shown = handled.slice(0, LIMIT);
  return (
    <section aria-label="Repositories" className="flex flex-col gap-1.5 text-xs">
      <h2 className="text-sm font-medium">Repositories</h2>
      <p className="text-muted-foreground">
        {all
          ? "All repositories, including ones added later."
          : `Only ${handled.length === 1 ? "this repository" : `these ${handled.length} repositories`}.`}
      </p>
      {shown.length > 0 ? (
        <ul className="flex flex-col gap-1">
          {shown.map((p) => (
            <li
              key={p.id}
              className="flex items-center gap-2 rounded-md border border-border px-2 py-1.5"
            >
              <ProjectAvatar project={p} className="size-4" />
              <span className="min-w-0 flex-1 truncate">{p.name}</span>
            </li>
          ))}
        </ul>
      ) : (
        <p className="text-muted-foreground">
          {all ? "There are no repositories yet." : "None of its repositories exist any more."}
        </p>
      )}
      {handled.length > LIMIT ? (
        <p className="text-muted-foreground">and {handled.length - LIMIT} more</p>
      ) : null}
      {gone > 0 ? (
        <p className="text-warning">
          {gone} picked {gone === 1 ? "repository was" : "repositories were"} removed from Circles.
        </p>
      ) : null}
    </section>
  );
}

/** The outside services a Composer can use: which ones it has on, and whether each one works. */
function IntegrationsSection({ composer }: { composer: Composer }) {
  const jira = useJiraStatus().data;
  const used = composer.integrations.includes("jira");
  const state = !jira?.connected
    ? { label: "Not connected", tone: "text-warning", dot: "bg-warning" }
    : jira.needs_reconnect
      ? { label: "Reconnect needed", tone: "text-warning", dot: "bg-warning" }
      : { label: "Connected", tone: "text-success", dot: "bg-success" };
  return (
    <section aria-label="Integrations" className="flex flex-col gap-1.5 text-xs">
      <h2 className="text-sm font-medium">Integrations</h2>
      <ul className="flex flex-col gap-1">
        <li className="flex items-center gap-2 rounded-md border border-border px-2 py-1.5">
          <JiraIcon className="size-4" />
          <span className="min-w-0 flex-1 truncate font-medium">Jira</span>
          {used ? (
            <Badge variant="outline" className={cn("gap-1.5", state.tone)}>
              <span aria-hidden className={cn("size-1.5 rounded-full", state.dot)} />
              {state.label}
            </Badge>
          ) : (
            <Badge variant="secondary">Off for this Composer</Badge>
          )}
        </li>
      </ul>
      {used && jira?.connected && jira.site ? (
        <p className="text-muted-foreground">{jira.site}</p>
      ) : null}
      {!used ? (
        <p className="text-muted-foreground">
          {jira?.connected
            ? "Jira is connected. Switch it on for this Composer in its Settings."
            : "Connect Jira in Settings, Integrations to use it."}
        </p>
      ) : null}
    </section>
  );
}

/** What the settings dialog edits. Nothing is saved until Save. */
type Draft = Pick<
  ComposerInput,
  "name" | "instructions" | "mode" | "projects" | "integrations" | "autonomy" | "limits" | "notes"
>;

const draftOf = (c: Composer): Draft => ({
  name: c.name,
  instructions: c.instructions,
  mode: c.mode,
  projects: c.projects,
  integrations: [...c.integrations],
  autonomy: { ...c.autonomy },
  limits: { ...c.limits },
  notes: c.notes,
});

const AUTONOMY_LABEL: Record<Autonomy, string> = {
  allow: "On its own",
  ask: "Asks you",
  off: "Off",
};

/** What the Composer does with each tool: on its own, asks you, or off. */
function AutonomyFields({
  draft,
  onChange,
}: {
  draft: Draft;
  onChange: (autonomy: Draft["autonomy"]) => void;
}) {
  const tools = useToolCatalog().data ?? [];
  const tiers: Tier[] = ["read", "write", "external", "destructive"];
  const set = (name: string, tier: Tier, value: Autonomy) => {
    const next: Record<string, Autonomy> = { ...draft.autonomy };
    // The same as its mode says is not an own setting.
    if (value === modeAutonomy(draft.mode ?? "manual", tier)) delete next[name];
    else next[name] = value;
    onChange(next);
  };
  return (
    <section aria-label="Autonomy" className="flex flex-col gap-4 text-xs">
      <div>
        <h3 className="text-sm font-medium">Autonomy</h3>
        <p className="mt-0.5 text-muted-foreground">
          What it may do on its own. Anything not set here follows its mode:{" "}
          {MODES.find((m) => m.value === draft.mode)?.label ?? draft.mode}.
        </p>
      </div>
      {tiers.map((tier) => {
        const group = tools.filter((t) => t.tier === tier);
        if (group.length === 0) return null;
        return (
          <fieldset key={tier} className="flex flex-col gap-1">
            <legend className="mb-1 font-medium">{TIER_LABEL[tier]}</legend>
            {group.map((t) => {
              const value = autonomyOf(
                { autonomy: draft.autonomy, mode: draft.mode ?? "manual" },
                t.name,
                tier,
              );
              return (
                <label
                  key={t.name}
                  className="flex items-center gap-2 rounded-md border border-border px-2 py-1"
                >
                  <span className="min-w-0 flex-1 truncate">{toolTitle(t.name)}</span>
                  <select
                    aria-label={toolTitle(t.name)}
                    value={value}
                    onChange={(e) => set(t.name, tier, e.target.value as Autonomy)}
                    className={cn(
                      "h-7 rounded-md border border-input bg-background px-1.5 text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring/50",
                      isRisky(tier) && value === "allow" && "text-warning",
                    )}
                  >
                    {(["allow", "ask", "off"] as const).map((a) => (
                      <option key={a} value={a}>
                        {AUTONOMY_LABEL[a]}
                      </option>
                    ))}
                  </select>
                </label>
              );
            })}
          </fieldset>
        );
      })}
    </section>
  );
}

/** The spending limit of one request. Nothing else is limited: only Claude Code's rate limits apply. */
function LimitsFields({
  limits,
  onChange,
}: {
  limits: Limits;
  onChange: (limits: Limits) => void;
}) {
  return (
    <section aria-label="Limits" className="flex flex-col gap-4 text-xs">
      <div>
        <h3 className="text-sm font-medium">Limits for one request</h3>
        <p className="mt-0.5 text-muted-foreground">
          No limit on time, steps or how many agents it starts.
        </p>
      </div>
      <div className="flex flex-col gap-1">
        <span className="font-medium">Spending limit in USD</span>
        <Input
          aria-label="Spending limit in USD"
          type="number"
          min={0}
          step="0.5"
          placeholder="No limit"
          value={limits.max_budget_usd ?? ""}
          onChange={(e) => {
            const n = Number(e.target.value);
            onChange({ ...limits, max_budget_usd: e.target.value === "" || n <= 0 ? null : n });
          }}
          className="h-8 w-28"
        />
        <span className="text-muted-foreground">Passed to Claude Code as its own limit.</span>
      </div>
    </section>
  );
}

/** The name, mode, instructions, repositories and integrations. */
function GeneralFields({
  draft,
  set,
  onClose,
}: {
  draft: Draft;
  set: <K extends keyof Draft>(key: K, value: Draft[K]) => void;
  onClose: () => void;
}) {
  const navigate = useNavigate();
  return (
    <section aria-label="General" className="flex min-w-0 flex-col gap-4">
      <NameModeFields
        name={draft.name}
        mode={draft.mode}
        onName={(v) => set("name", v)}
        onMode={(v) => set("mode", v)}
      />
      <InstructionsField value={draft.instructions} onChange={(v) => set("instructions", v)} />
      <RepositoriesField value={draft.projects} onChange={(v) => set("projects", v)} />
      <IntegrationsField
        value={draft.integrations}
        onChange={(v) => set("integrations", v)}
        onSettings={() => {
          onClose();
          navigate("/settings/integrations");
        }}
      />
    </section>
  );
}

type SettingsSection = "general" | "autonomy" | "limits" | "notes" | "triggers" | "delete";

const SETTINGS_SECTIONS: { id: SettingsSection; label: string }[] = [
  { id: "general", label: "General" },
  { id: "autonomy", label: "Autonomy" },
  { id: "limits", label: "Limits" },
  { id: "notes", label: "Notes" },
  { id: "triggers", label: "Starts on its own" },
  { id: "delete", label: "Delete" },
];

/** Everything that sets how a Composer behaves, in one dialog. The Details tab only shows facts. */
function ComposerSettingsDialog({
  composer,
  open,
  onOpenChange,
}: {
  composer: Composer;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const remove = useDeleteComposer();
  const update = useUpdateComposer();
  const navigate = useNavigate();
  const [confirming, setConfirming] = useState(false);
  const [section, setSection] = useState<SettingsSection>("general");
  const [draft, setDraft] = useState<Draft>(() => draftOf(composer));
  const [error, setError] = useState<string | null>(null);
  const saved = JSON.stringify(draftOf(composer));
  // A saved change, or another Composer, refills the draft.
  // biome-ignore lint/correctness/useExhaustiveDependencies: refilled when the saved copy changes
  useEffect(() => setDraft(draftOf(composer)), [saved]);
  const set = <K extends keyof Draft>(key: K, value: Draft[K]) =>
    setDraft((d) => ({ ...d, [key]: value }));
  const changed = JSON.stringify(draft) !== saved;
  const valid = draft.name.trim() !== "" && draft.projects?.length !== 0;
  const save = () => {
    setError(null);
    update.mutate(
      {
        id: composer.id,
        input: {
          ...draft,
          allow_override: composer.allow_override,
          model: composer.model,
          effort: composer.effort,
        },
      },
      {
        onSuccess: () => toast("Composer saved"),
        onError: (e) => setError((e as { message?: string })?.message ?? String(e)),
      },
    );
  };
  const content: Record<SettingsSection, React.ReactNode> = {
    general: <GeneralFields draft={draft} set={set} onClose={() => onOpenChange(false)} />,
    autonomy: <AutonomyFields draft={draft} onChange={(v) => set("autonomy", v)} />,
    limits: <LimitsFields limits={draft.limits} onChange={(v) => set("limits", v)} />,
    notes: <Notes value={draft.notes} onChange={(v) => set("notes", v)} />,
    triggers: <TriggersEditor composerId={composer.id} />,
    delete: (
      <section aria-label="Delete" className="flex flex-col gap-2 text-xs">
        <h3 className="text-sm font-medium">Delete</h3>
        <p className="text-muted-foreground">
          Its runs and history are deleted. Workspaces its runs started are kept.
        </p>
        <div>
          <Button size="sm" variant="outline" onClick={() => setConfirming(true)}>
            <Trash2Icon />
            Delete Composer
          </Button>
        </div>
      </section>
    ),
  };
  return (
    <>
      <Dialog open={open} onOpenChange={onOpenChange}>
        <DialogContent className="flex h-[70vh] max-h-[640px] flex-col gap-0 p-0 sm:max-w-3xl">
          <DialogHeader className="shrink-0 border-b border-border px-5 py-3">
            <DialogTitle>{composer.name} settings</DialogTitle>
            <DialogDescription className="sr-only">
              How this Composer works, when it starts on its own, and what it remembers.
            </DialogDescription>
          </DialogHeader>
          <div className="flex min-h-0 flex-1">
            <nav
              aria-label="Composer settings"
              className="flex w-44 shrink-0 flex-col gap-0.5 border-r border-border p-2"
            >
              {SETTINGS_SECTIONS.map((item) => (
                <button
                  key={item.id}
                  type="button"
                  aria-current={section === item.id ? "page" : undefined}
                  onClick={() => setSection(item.id)}
                  className={cn(
                    "rounded-md px-2 py-1.5 text-left text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
                    section === item.id
                      ? "bg-muted font-medium text-foreground"
                      : "text-muted-foreground hover:bg-muted/60 hover:text-foreground",
                    item.id === "delete" && section !== item.id && "text-destructive",
                  )}
                >
                  {item.label}
                </button>
              ))}
            </nav>
            <div className="min-w-0 flex-1 overflow-y-auto p-5 text-sm">{content[section]}</div>
          </div>
          {changed ? (
            <DialogFooter className="mx-0 mb-0 shrink-0 flex-row items-center justify-end rounded-none border-t border-border px-5 py-3">
              {error ? (
                <p role="alert" className="mr-auto text-sm text-destructive">
                  {error}
                </p>
              ) : null}
              <Button
                variant="outline"
                disabled={update.isPending}
                onClick={() => {
                  setError(null);
                  setDraft(draftOf(composer));
                }}
              >
                Cancel
              </Button>
              <Button disabled={update.isPending || !valid} onClick={save}>
                {update.isPending ? <Loader2Icon className="animate-spin" /> : null}
                Save
              </Button>
            </DialogFooter>
          ) : null}
        </DialogContent>
      </Dialog>

      <Dialog open={confirming} onOpenChange={setConfirming}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Delete {composer.name}?</DialogTitle>
            <DialogDescription>
              Its runs and history are deleted. Workspaces its runs started are kept.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setConfirming(false)}>
              Cancel
            </Button>
            <Button
              variant="destructive"
              disabled={remove.isPending}
              onClick={() =>
                remove.mutate(composer.id, {
                  onSuccess: () => {
                    setConfirming(false);
                    onOpenChange(false);
                    navigate("/composers");
                  },
                  onError: (e) => toast.error(e instanceof Error ? e.message : String(e)),
                })
              }
            >
              Delete
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

/** The facts about a Composer: its name, the repositories it handles and its integrations. */
function DetailsTab({ composer }: { composer: Composer }) {
  const [settings, setSettings] = useState(false);
  return (
    <div className="flex h-full min-h-0 flex-col text-sm">
      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-3">
        <RepositoriesSection composer={composer} />
        <IntegrationsSection composer={composer} />
      </div>
      <div className="shrink-0 border-t border-border p-3">
        <Button
          size="sm"
          variant="outline"
          aria-label="Composer settings"
          onClick={() => setSettings(true)}
        >
          <SettingsIcon />
          Settings
        </Button>
      </div>
      <ComposerSettingsDialog composer={composer} open={settings} onOpenChange={setSettings} />
    </div>
  );
}

/** The right pane of a Composer: what it started, what waits for you, what it did, and its setup. */
export function ComposerPanel({
  composer,
  runId,
  initialTab = "fleet",
}: {
  composer: Composer;
  runId?: string;
  /** The tab it opens on. */
  initialTab?: Tab;
}) {
  const [tab, setTab] = useState<Tab>(initialTab);
  const pending = (useApprovals({ composerId: composer.id, onlyPending: true }).data ?? []).length;
  return (
    <div className="flex h-full min-h-0 flex-col bg-background">
      <div
        role="tablist"
        aria-label="Composer"
        className="pointer-events-auto relative z-30 flex h-10 shrink-0 items-center gap-1 overflow-x-auto border-b border-border px-3 [scrollbar-width:none]"
      >
        {TABS.map(({ id, label }) => (
          <button
            key={id}
            type="button"
            role="tab"
            aria-selected={tab === id}
            onClick={() => setTab(id)}
            className={
              tab === id
                ? "shrink-0 rounded-md bg-muted px-2 py-1 text-xs whitespace-nowrap text-foreground"
                : "shrink-0 rounded-md px-2 py-1 text-xs whitespace-nowrap text-muted-foreground hover:text-foreground"
            }
          >
            {label}
            {id === "approvals" && pending > 0 ? (
              <span className="ml-1 rounded-full bg-warning/20 px-1.5 text-warning">{pending}</span>
            ) : null}
          </button>
        ))}
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto">
        {tab === "fleet" ? <FleetTab composer={composer} runId={runId} /> : null}
        {tab === "approvals" ? <ApprovalsTab composer={composer} /> : null}
        {tab === "steps" ? <StepsTab composer={composer} runId={runId} /> : null}
        {tab === "details" ? <DetailsTab composer={composer} /> : null}
      </div>
    </div>
  );
}
