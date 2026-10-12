import type { SettingsView } from "@circles/bindings/SettingsView";
import type { SmartTest } from "@circles/bindings/SmartTest";
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
import { useQuery } from "@tanstack/react-query";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Loader2Icon, PlusIcon, Trash2Icon } from "lucide-react";
import { type ReactNode, useState } from "react";
import { Link } from "react-router";
import { toast } from "sonner";
import { useComposersStatus } from "@/features/composers/api";
import { PauseButton, StopAllButton } from "@/features/composers/stop-all";
import { useJiraStatus } from "@/features/integrations/api";
import { JiraCard } from "@/features/integrations/jira-card";
import { useOnboarding } from "@/features/onboarding/store";
import { ipc } from "@/lib/ipc";
import { layerValue, useSetSetting } from "./api";
import { SETTINGS, type SettingDef } from "./defs";
import { DefRow, SettingRow } from "./rows";

const message = (e: unknown) =>
  e instanceof Error ? e.message : ((e as { message?: string })?.message ?? String(e));

const FULL_DISK_URL = "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles";

function AccountRow({ def, highlight }: { def: SettingDef; highlight?: string }) {
  return (
    <SettingRow title={def.title} description={def.description} highlight={highlight}>
      <span className="text-xs text-muted-foreground">Not signed in</span>
    </SettingRow>
  );
}

function TestRow({ def, highlight }: { def: SettingDef; highlight?: string }) {
  const run = async (fn: () => Promise<unknown>) => {
    try {
      await fn();
    } catch (e) {
      toast.error(message(e));
    }
  };
  return (
    <SettingRow title={def.title} description={def.description} highlight={highlight}>
      <Button variant="outline" size="sm" onClick={() => void run(ipc.notifyTest)}>
        Send test
      </Button>
      <Button variant="outline" size="sm" onClick={() => void run(ipc.soundPlay)}>
        Play sound
      </Button>
    </SettingRow>
  );
}

function FullDiskRow({ def, highlight }: { def: SettingDef; highlight?: string }) {
  return (
    <SettingRow title={def.title} description={def.description} highlight={highlight}>
      <Button
        variant="outline"
        size="sm"
        onClick={() => openUrl(FULL_DISK_URL).catch((e) => toast.error(message(e)))}
      >
        Open privacy settings
      </Button>
    </SettingRow>
  );
}

function IdentityRow({ def, highlight }: { def: SettingDef; highlight?: string }) {
  const identity = useQuery({ queryKey: ["git-identity"], queryFn: ipc.gitIdentity });
  const { name, email } = identity.data ?? {};
  return (
    <SettingRow title={def.title} description={def.description} highlight={highlight}>
      <span className="text-xs text-muted-foreground">
        {name || email ? `${name ?? ""} ${email ? `<${email}>` : ""}`.trim() : "Not configured"}
      </span>
    </SettingRow>
  );
}

function ClaudeCheckRow({ def, highlight }: { def: SettingDef; highlight?: string }) {
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<Awaited<ReturnType<typeof ipc.claudeCheck>> | null>(null);
  const [error, setError] = useState<string | null>(null);
  const check = async () => {
    setBusy(true);
    setError(null);
    try {
      setResult(await ipc.claudeCheck());
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <SettingRow
      title={def.title}
      description={def.description}
      highlight={highlight}
      error={error ?? result?.problem}
    >
      {result?.found ? (
        <span className="text-xs text-success" data-testid="claude-version">
          {result.version ?? "Found"}
        </span>
      ) : null}
      <Button variant="outline" size="sm" disabled={busy} onClick={() => void check()}>
        {busy ? <Loader2Icon className="animate-spin" /> : null}
        Check
      </Button>
    </SettingRow>
  );
}

/** Environment variables of the user file, with the values hidden. */
function EnvRow({
  def,
  view,
  highlight,
}: {
  def: SettingDef;
  view: SettingsView;
  highlight?: string;
}) {
  const set = useSetSetting();
  const own = (layerValue(view, "user", ["environment_variables"]) ?? {}) as Record<string, string>;
  const [name, setName] = useState("");
  const [value, setValue] = useState("");
  const [revealed, setRevealed] = useState<string | null>(null);
  const add = () => {
    const key = name.trim();
    if (!key) return;
    set.mutate({ scope: "user", path: ["environment_variables", key], value });
    setName("");
    setValue("");
  };
  const error = set.error ? message(set.error) : null;
  return (
    <SettingRow
      title={def.title}
      description={def.description}
      highlight={highlight}
      stacked
      error={error}
    >
      <div className="flex w-full flex-col gap-2">
        {Object.entries(own).map(([key, secret]) => (
          <div key={key} className="flex items-center gap-2 font-mono text-xs">
            <span className="w-48 truncate">{key}</span>
            <span className="min-w-0 flex-1 truncate text-muted-foreground">
              {revealed === key ? secret : "••••••••"}
            </span>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setRevealed(revealed === key ? null : key)}
            >
              {revealed === key ? "Hide" : "Show"}
            </Button>
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={`Remove ${key}`}
              onClick={() =>
                set.mutate({
                  scope: "user",
                  path: ["environment_variables", key],
                  value: undefined,
                })
              }
            >
              <Trash2Icon />
            </Button>
          </div>
        ))}
        <div className="flex items-center gap-2">
          <Input
            aria-label="Variable name"
            placeholder="NAME"
            value={name}
            onChange={(e) => setName(e.target.value)}
            className="h-8 w-48 font-mono text-xs"
          />
          <Input
            aria-label="Variable value"
            type="password"
            placeholder="value"
            value={value}
            onChange={(e) => setValue(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && add()}
            className="h-8 flex-1 font-mono text-xs"
          />
          <Button variant="outline" size="sm" onClick={add} disabled={!name.trim()}>
            <PlusIcon />
            Add
          </Button>
        </div>
      </div>
    </SettingRow>
  );
}

function JiraAccountRow({ def, highlight }: { def: SettingDef; highlight?: string }) {
  const status = useJiraStatus().data;
  return (
    <SettingRow title={def.title} description={def.description} highlight={highlight}>
      <div className="flex flex-col items-end gap-0.5 text-xs">
        <span data-testid="jira-account">
          {status?.connected
            ? `${status.account_name} on ${status.site} (${status.method === "oauth" ? "signed in with Atlassian" : "API token"})`
            : "Not connected"}
        </span>
        <Link className="text-muted-foreground underline" to="/settings/integrations">
          Reconnect or disconnect
        </Link>
      </div>
    </SettingRow>
  );
}

function SmartTestRow({ def, highlight }: { def: SettingDef; highlight?: string }) {
  const [result, setResult] = useState<SmartTest | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const status = useQuery({ queryKey: ["smart-status"], queryFn: ipc.smartStatus });
  const run = async () => {
    setBusy(true);
    setError(null);
    try {
      setResult(await ipc.smartTest());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <SettingRow title={def.title} description={def.description} highlight={highlight} stacked>
      <div className="flex w-full flex-col gap-2 text-xs">
        <div className="flex items-center gap-2">
          <Button size="sm" variant="outline" disabled={busy} onClick={run}>
            {busy ? "Running…" : "Try it"}
          </Button>
          {status.data && !status.data.embedded_available && status.data.provider === "embedded" ? (
            <span role="alert" className="text-warning">
              The built-in model file was not found in this copy of Circles.
            </span>
          ) : null}
        </div>
        {error ? (
          <p role="alert" className="text-destructive">
            {error}
          </p>
        ) : null}
        {result ? (
          <dl data-testid="smart-result" className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1">
            <dt className="text-muted-foreground">Title</dt>
            <dd>{result.title ?? "No answer"}</dd>
            <dt className="text-muted-foreground">Next message</dt>
            <dd>{result.suggestion ?? "No answer"}</dd>
            <dt className="text-muted-foreground">Time</dt>
            <dd>{result.millis} ms</dd>
          </dl>
        ) : null}
      </div>
    </SettingRow>
  );
}

function ComposersStateRow({ def, highlight }: { def: SettingDef; highlight?: string }) {
  const status = useComposersStatus().data;
  return (
    <SettingRow title={def.title} description={def.description} highlight={highlight} stacked>
      <div className="flex w-full flex-wrap items-center gap-2">
        <PauseButton paused={status?.paused ?? false} />
        <StopAllButton />
        <span className="text-xs text-muted-foreground">
          {status?.paused ? "Paused. " : ""}
          {status ? `${status.running} running, ${status.waiting_approvals} waiting for you.` : ""}
        </span>
      </div>
    </SettingRow>
  );
}

function ExperimentalRow({ def, highlight }: { def: SettingDef; highlight?: string }) {
  return (
    <SettingRow title={def.title} description={def.description} highlight={highlight}>
      <span className="text-xs text-muted-foreground">Nothing to switch on yet</span>
    </SettingRow>
  );
}

function PathsRow({ def, highlight }: { def: SettingDef; highlight?: string }) {
  const info = useQuery({ queryKey: ["app-info"], queryFn: ipc.getAppInfo });
  const paths = info.data?.paths;
  return (
    <SettingRow title={def.title} description={def.description} highlight={highlight} stacked>
      <div className="flex w-full flex-col gap-1">
        {paths
          ? [
              ["Data", paths.data_dir],
              ["Settings", paths.settings_file],
              ["Logs", paths.logs_dir],
              ["Workspaces", paths.workspaces_dir],
            ].map(([label, path]) => (
              <p key={label} className="font-mono text-xs text-muted-foreground">
                <span className="mr-2 inline-block w-20 text-foreground">{label}</span>
                {path}
              </p>
            ))
          : null}
        <Button
          variant="outline"
          size="sm"
          className="mt-1 w-fit"
          onClick={() => ipc.dataReveal().catch((e) => toast.error(message(e)))}
        >
          Reveal in Finder
        </Button>
      </div>
    </SettingRow>
  );
}

function ExportRow({ def, highlight }: { def: SettingDef; highlight?: string }) {
  const [busy, setBusy] = useState(false);
  return (
    <SettingRow title={def.title} description={def.description} highlight={highlight}>
      <Button
        variant="outline"
        size="sm"
        disabled={busy}
        onClick={async () => {
          setBusy(true);
          try {
            const folder = await ipc.logsExport();
            toast(`Logs exported to ${folder}`);
          } catch (e) {
            toast.error(message(e));
          } finally {
            setBusy(false);
          }
        }}
      >
        {busy ? <Loader2Icon className="animate-spin" /> : null}
        Export logs
      </Button>
    </SettingRow>
  );
}

function DiagnosticsRow({ def, highlight }: { def: SettingDef; highlight?: string }) {
  return (
    <SettingRow title={def.title} description={def.description} highlight={highlight}>
      <Button
        variant="outline"
        size="sm"
        onClick={async () => {
          try {
            await navigator.clipboard.writeText(await ipc.diagnosticsReport());
            toast("Diagnostics copied");
          } catch (e) {
            toast.error(message(e));
          }
        }}
      >
        Copy diagnostics
      </Button>
    </SettingRow>
  );
}

function SetupAgainRow({ def, highlight }: { def: SettingDef; highlight?: string }) {
  const again = useOnboarding((s) => s.again);
  return (
    <SettingRow title={def.title} description={def.description} highlight={highlight}>
      <Button variant="outline" size="sm" onClick={again}>
        Show again
      </Button>
    </SettingRow>
  );
}

function ResetRow({ def, highlight }: { def: SettingDef; highlight?: string }) {
  const [open, setOpen] = useState(false);
  const [word, setWord] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const reset = async () => {
    setBusy(true);
    setError(null);
    try {
      await ipc.appResetData(word);
    } catch (e) {
      setError(message(e));
      setBusy(false);
    }
  };
  return (
    <>
      <SettingRow title={def.title} description={def.description} highlight={highlight}>
        <Button variant="destructive" size="sm" onClick={() => setOpen(true)}>
          Reset…
        </Button>
      </SettingRow>
      <Dialog
        open={open}
        onOpenChange={(next) => {
          setOpen(next);
          if (!next) {
            setWord("");
            setError(null);
          }
        }}
      >
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Reset app data?</DialogTitle>
            <DialogDescription>
              This stops every agent and terminal, then deletes chats, workspace records,
              checkpoints and terminal history. Your settings, logs and repositories are kept.
              Circles restarts afterwards. This cannot be undone.
            </DialogDescription>
          </DialogHeader>
          <Input
            aria-label="Type reset to confirm"
            placeholder="Type reset to confirm"
            value={word}
            onChange={(e) => setWord(e.target.value)}
          />
          {error ? (
            <p role="alert" className="text-sm text-destructive">
              {error}
            </p>
          ) : null}
          <DialogFooter>
            <Button variant="outline" onClick={() => setOpen(false)}>
              Cancel
            </Button>
            <Button
              variant="destructive"
              disabled={word.trim().toLowerCase() !== "reset" || busy}
              onClick={() => void reset()}
            >
              {busy ? <Loader2Icon className="animate-spin" /> : null}
              Reset app data
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

/** A row that is not a plain value in the file. */
export function CustomRow({
  def,
  view,
  highlight,
}: {
  def: SettingDef;
  view: SettingsView;
  highlight?: string;
}) {
  switch (def.id) {
    case "account":
      return <AccountRow def={def} highlight={highlight} />;
    case "sound_test":
      return <TestRow def={def} highlight={highlight} />;
    case "full_disk_access":
      return <FullDiskRow def={def} highlight={highlight} />;
    case "identity":
      return <IdentityRow def={def} highlight={highlight} />;
    case "claude_check":
      return <ClaudeCheckRow def={def} highlight={highlight} />;
    case "env_vars":
      return <EnvRow def={def} view={view} highlight={highlight} />;
    case "smart_notice":
      return (
        <SettingRow title={def.title} description={def.description} highlight={highlight}>
          {null}
        </SettingRow>
      );
    case "smart_test":
      return <SmartTestRow def={def} highlight={highlight} />;
    case "jira_card":
      return <JiraCard />;
    case "jira_account":
      return <JiraAccountRow def={def} highlight={highlight} />;
    case "composers_state":
      return <ComposersStateRow def={def} highlight={highlight} />;
    case "experimental":
      return <ExperimentalRow def={def} highlight={highlight} />;
    case "data_paths":
      return <PathsRow def={def} highlight={highlight} />;
    case "export_logs":
      return <ExportRow def={def} highlight={highlight} />;
    case "diagnostics":
      return <DiagnosticsRow def={def} highlight={highlight} />;
    case "setup_again":
      return <SetupAgainRow def={def} highlight={highlight} />;
    case "reset":
      return <ResetRow def={def} highlight={highlight} />;
    default:
      return null;
  }
}

/** One row of a definition, whichever kind it is. */
export function Row({
  def,
  view,
  highlight,
}: {
  def: SettingDef;
  view: SettingsView;
  highlight?: string;
}) {
  return def.control.kind === "custom" ? (
    <CustomRow def={def} view={view} highlight={highlight} />
  ) : (
    <DefRow def={def} view={view} highlight={highlight} />
  );
}

/** The rows of one page, with the warning that goes with bypassing permissions. */
export function SettingsGroupPage({ page, view }: { page: string; view: SettingsView }): ReactNode {
  const defs = SETTINGS.filter((d) => d.page === page);
  const bypass = view.settings.agents.permission_mode === "bypass";
  return (
    <div>
      {defs.map((def) => (
        <div key={def.id}>
          <Row def={def} view={view} />
          {def.id === "permission_mode" && bypass ? (
            <p
              role="alert"
              className="mb-2 rounded-md border border-warning/50 bg-warning/10 p-2 text-xs"
            >
              Bypass lets the agent run any command and change any file without asking. Only use it
              in workspaces you can throw away.
            </p>
          ) : null}
        </div>
      ))}
    </div>
  );
}
