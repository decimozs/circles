import { Input } from "@circles/ui/components/input";
import { Switch } from "@circles/ui/components/switch";
import { cn } from "@circles/ui/utils";
import { useState } from "react";
import { JiraIcon } from "@/components/jira-icon";
import { Markdown } from "@/features/chat/markdown";
import { useJiraStatus } from "@/features/integrations/api";
import { MODES } from "./api";
import { RepoPicker } from "./repo-picker";
import { SkillTextarea } from "./skill-textarea";

/** Which repositories the Composer may work on: all of them, or only the ones picked. */
export function RepositoriesField({
  value,
  onChange,
}: {
  value: string[] | null;
  onChange: (projects: string[] | null) => void;
}) {
  const only = value !== null;

  return (
    <fieldset className="flex min-w-0 flex-col gap-2 text-sm">
      <legend className="mb-1 font-medium">Repositories</legend>
      <select
        aria-label="Repositories it may work on"
        value={only ? "only" : "all"}
        onChange={(e) => onChange(e.target.value === "all" ? null : [])}
        className="h-8 w-full min-w-0 rounded-md border border-input bg-background px-2 text-xs outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
      >
        <option value="all">All repositories, including ones added later</option>
        <option value="only">Only the repositories I pick</option>
      </select>
      {only ? (
        <>
          <RepoPicker value={value ?? []} onChange={onChange} />
          {value?.length === 0 ? (
            <p className="text-xs text-warning">Pick at least one repository.</p>
          ) : (
            <p className="text-xs text-muted-foreground">
              {value?.length} picked. It cannot see or change any other.
            </p>
          )}
        </>
      ) : null}
    </fieldset>
  );
}

/** Outside services this Composer may use. Each one is off until it is switched on here. */
export function IntegrationsField({
  value,
  onChange,
  onSettings,
}: {
  value: string[];
  onChange: (integrations: string[]) => void;
  onSettings: () => void;
}) {
  const jira = useJiraStatus().data;
  const connected = !!jira?.connected && !jira.needs_reconnect;
  const on = value.includes("jira");
  return (
    <fieldset className="flex min-w-0 flex-col gap-2 text-sm">
      <legend className="mb-1 font-medium">Integrations</legend>
      <div className="flex items-center gap-3 rounded-md border border-border p-2">
        <JiraIcon className="size-5" />
        <div className="min-w-0 flex-1">
          <div className="text-xs font-medium">Jira</div>
          <p className="text-xs text-muted-foreground">
            {connected ? (
              `Search and read issues. Comments, moves and edits follow its mode.`
            ) : (
              <>
                {jira?.needs_reconnect
                  ? "Jira needs to be reconnected. "
                  : "Jira is not connected. "}
                <button type="button" className="underline" onClick={onSettings}>
                  Open Settings, Integrations
                </button>
              </>
            )}
          </p>
        </div>
        <Switch
          aria-label="Use Jira"
          checked={on && connected}
          disabled={!connected}
          onCheckedChange={(next) =>
            onChange(
              next
                ? [...value.filter((v) => v !== "jira"), "jira"]
                : value.filter((v) => v !== "jira"),
            )
          }
        />
      </div>
    </fieldset>
  );
}

/** The name of a Composer and the permission mode it runs in. */
export function NameModeFields({
  name,
  mode,
  onName,
  onMode,
}: {
  name: string;
  mode: string | null;
  onName: (name: string) => void;
  onMode: (mode: string) => void;
}) {
  const current = MODES.find((m) => m.value === mode) ?? MODES[0];
  return (
    <>
      <div className="grid grid-cols-[4fr_1fr] gap-3">
        <div className="flex min-w-0 flex-col gap-1 text-sm">
          <span className="font-medium">Name</span>
          <Input
            aria-label="Name"
            value={name}
            maxLength={80}
            onChange={(e) => onName(e.target.value)}
            placeholder="Release captain"
          />
        </div>
        <div className="flex min-w-0 flex-col gap-1 text-sm">
          <span className="font-medium">Mode</span>
          <select
            aria-label="Mode"
            title={current.hint}
            value={current.value}
            onChange={(e) => onMode(e.target.value)}
            className={cn(
              "h-8 w-full min-w-0 rounded-md border border-input bg-background px-2 text-xs outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50",
              current.value === "bypass" && "text-destructive",
            )}
          >
            {MODES.map((m) => (
              <option key={m.value} value={m.value}>
                {m.label}
              </option>
            ))}
          </select>
        </div>
      </div>
      <p
        className={cn(
          "-mt-2 text-xs text-muted-foreground",
          current.value === "bypass" && "text-destructive",
        )}
      >
        {current.hint}
      </p>
    </>
  );
}

/** The instructions, written as Markdown with a preview. */
export function InstructionsField({
  value,
  onChange,
}: {
  value: string;
  onChange: (value: string) => void;
}) {
  const [view, setView] = useState<"write" | "preview">("write");
  return (
    <div className="flex flex-col gap-1.5 text-sm">
      <div className="flex items-center justify-between gap-2">
        <span className="font-medium">Instructions</span>
        <div className="flex items-center gap-2">
          <span className="text-xs text-muted-foreground">Markdown</span>
          <fieldset aria-label="Instructions view" className="flex rounded-md bg-muted p-0.5">
            {(["write", "preview"] as const).map((mode) => (
              <button
                key={mode}
                type="button"
                aria-pressed={view === mode}
                onClick={() => setView(mode)}
                className={
                  view === mode
                    ? "rounded bg-background px-2 py-0.5 text-xs text-foreground shadow-sm"
                    : "rounded px-2 py-0.5 text-xs text-muted-foreground"
                }
              >
                {mode === "write" ? "Write" : "Preview"}
              </button>
            ))}
          </fieldset>
        </div>
      </div>
      {view === "write" ? (
        <SkillTextarea
          aria-label="Instructions"
          value={value}
          onValueChange={onChange}
          placeholder="What it should do, how it should work, and what it must never do. Markdown works. Type / to use one of your skills."
        />
      ) : (
        <section
          aria-label="Instructions preview"
          className="max-h-72 min-h-44 overflow-y-auto rounded-lg border border-input px-3 py-2"
        >
          {value.trim() ? (
            <Markdown text={value} />
          ) : (
            <p className="text-sm text-muted-foreground">Nothing to preview yet.</p>
          )}
        </section>
      )}
    </div>
  );
}
