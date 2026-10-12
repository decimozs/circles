import type { SettingsView } from "@circles/bindings/SettingsView";
import { Button } from "@circles/ui/components/button";
import { Input } from "@circles/ui/components/input";
import { Switch } from "@circles/ui/components/switch";
import { Textarea } from "@circles/ui/components/textarea";
import { cn } from "@circles/ui/utils";
import { RotateCcwIcon } from "lucide-react";
import { type ReactNode, useEffect, useState } from "react";
import { layerValue, useSetSetting, valueAt, type WriteScope } from "./api";
import type { SettingDef } from "./defs";

/** One row: what the setting is on the left, the control on the right. */
export function SettingRow({
  title,
  description,
  htmlFor,
  children,
  error,
  highlight,
  stacked,
  action,
  hint,
}: {
  title: string;
  description: string;
  htmlFor?: string;
  children: ReactNode;
  error?: string | null;
  highlight?: string;
  stacked?: boolean;
  action?: ReactNode;
  hint?: string;
}) {
  return (
    <div
      data-testid="setting-row"
      className={cn(
        "flex gap-6 border-b border-border py-5 last:border-b-0",
        stacked ? "flex-col" : "items-start justify-between",
      )}
    >
      <div className="min-w-0 flex-1">
        <label htmlFor={htmlFor} className="text-[15px] font-medium">
          <Marked text={title} query={highlight} />
        </label>
        <p className="mt-1.5 text-sm text-muted-foreground">
          <Marked text={description} query={highlight} />
        </p>
        {hint ? <p className="mt-1.5 text-xs text-muted-foreground">{hint}</p> : null}
        {error ? (
          <p role="alert" className="mt-1 text-xs text-destructive">
            {error}
          </p>
        ) : null}
      </div>
      <div className={cn("flex shrink-0 items-center gap-2", stacked && "w-full")}>
        {action}
        {children}
      </div>
    </div>
  );
}

/** Text with the searched words highlighted. */
function Marked({ text, query }: { text: string; query?: string }) {
  const words = (query ?? "").split(/\s+/).filter(Boolean);
  if (words.length === 0) return <>{text}</>;
  const escaped = words.map((w) => w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
  const parts = text.split(new RegExp(`(${escaped.join("|")})`, "gi"));
  return (
    <>
      {parts.map((part, i) =>
        i % 2 === 1 ? (
          // biome-ignore lint/suspicious/noArrayIndexKey: parts have no identity
          <mark key={i} className="rounded-sm bg-warning/30 text-foreground">
            {part}
          </mark>
        ) : (
          part
        ),
      )}
    </>
  );
}

const SELECT =
  "h-9 min-w-32 rounded-md border border-input bg-background px-2 text-sm outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50";

/** A text-like control that saves when it loses focus, and puts back what the file holds. */
function Draft({
  value,
  onCommit,
  multiline,
  id,
  placeholder,
  mono,
  label,
}: {
  value: string;
  onCommit: (value: string) => void;
  multiline?: boolean;
  id: string;
  placeholder?: string;
  mono?: boolean;
  label: string;
}) {
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  const commit = () => {
    if (draft !== value) onCommit(draft);
  };
  if (multiline) {
    return (
      <Textarea
        id={id}
        aria-label={label}
        value={draft}
        placeholder={placeholder}
        rows={4}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        className={cn("w-full text-xs", mono && "font-mono")}
      />
    );
  }
  return (
    <Input
      id={id}
      aria-label={label}
      value={draft}
      placeholder={placeholder}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") commit();
      }}
      className={cn("h-8 w-64 text-xs", mono && "font-mono")}
    />
  );
}

/** A row for a setting in the file, built from its definition. */
export function DefRow({
  def,
  view,
  projectId,
  scope = "user",
  highlight,
}: {
  def: SettingDef;
  view: SettingsView;
  projectId?: string;
  /** The file a change is written to. */
  scope?: WriteScope;
  highlight?: string;
}) {
  const set = useSetSetting(projectId);
  const effective = valueAt(view.settings, def.path);
  const layer = scope === "local" ? "local_project" : scope;
  const own = layerValue(view, layer, def.path);
  const write = (value: unknown) => set.mutate({ scope, path: def.path, value });
  const id = `setting-${def.id}`;
  const c = def.control;
  const error = set.error ? (set.error as { message?: string }).message || String(set.error) : null;

  let control: ReactNode = null;
  if (c.kind === "toggle") {
    control = (
      <Switch
        id={id}
        aria-label={def.title}
        checked={typeof effective === "boolean" ? effective : c.default}
        onCheckedChange={(checked) => write(checked)}
      />
    );
  } else if (c.kind === "select") {
    const current =
      typeof effective === "string" || typeof effective === "number"
        ? String(effective)
        : (c.default ?? "");
    control = (
      <select
        id={id}
        aria-label={def.title}
        className={SELECT}
        value={current}
        onChange={(e) => write(c.numeric ? Number(e.target.value) : e.target.value)}
      >
        {c.default === undefined && !current ? <option value="">Not set</option> : null}
        {c.options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
    );
  } else if (c.kind === "text" || c.kind === "textarea") {
    control = (
      <Draft
        id={id}
        label={def.title}
        value={typeof effective === "string" ? effective : ""}
        placeholder={c.placeholder}
        mono={c.mono}
        multiline={c.kind === "textarea"}
        onCommit={(text) => write(text.trim() === "" ? undefined : text)}
      />
    );
  } else if (c.kind === "list") {
    const items = Array.isArray(effective) ? (effective as string[]) : [];
    control = (
      <Draft
        id={id}
        label={def.title}
        value={items.join("\n")}
        placeholder={c.placeholder}
        mono
        multiline
        onCommit={(text) => {
          const lines = text
            .split("\n")
            .map((l) => l.trim())
            .filter(Boolean);
          write(lines.length === 0 ? undefined : lines);
        }}
      />
    );
  }

  const reset =
    own !== undefined ? (
      <Button
        variant="ghost"
        size="icon-sm"
        aria-label={`Reset ${def.title}`}
        title="Back to the default"
        onClick={() => write(undefined)}
      >
        <RotateCcwIcon />
      </Button>
    ) : null;

  return (
    <SettingRow
      title={def.title}
      description={def.description}
      htmlFor={id}
      error={error}
      highlight={highlight}
      stacked={c.kind === "textarea" || c.kind === "list"}
      action={reset}
      hint={def.hint}
    >
      {control}
    </SettingRow>
  );
}
