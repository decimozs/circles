import type { ScriptsInfo } from "@circles/bindings/ScriptsInfo";
import type { Terminal } from "@circles/bindings/Terminal";
import type { Workspace } from "@circles/bindings/Workspace";
import { Button } from "@circles/ui/components/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@circles/ui/components/dropdown-menu";
import { Tooltip, TooltipContent, TooltipTrigger } from "@circles/ui/components/tooltip";
import { cn } from "@circles/ui/utils";
import {
  ChevronDownIcon,
  FileCogIcon,
  Loader2Icon,
  Maximize2Icon,
  Minimize2Icon,
  PlayIcon,
  PlusIcon,
  SquareIcon,
  XIcon,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import {
  useCreateTerminal,
  useDeleteTerminal,
  useRunScript,
  useRunSetup,
  useScripts,
  useStopScript,
  useTerminals,
} from "@/features/terminal/api";
import { ScriptIcon } from "@/features/terminal/script-icon";
import { TerminalView } from "@/features/terminal/terminal-view";
import { ipc } from "@/lib/ipc";
import { useUi } from "@/stores/ui";

function openSettings(projectId: string) {
  ipc.projectOpenSettings(projectId).catch((error: Error) => toast.error(error.message));
}

/**
 * Setup and Run tabs show progress while their script runs. A free terminal is a shell that stays
 * open for as long as the tab does, so it shows nothing: a spinner there would never stop.
 */
function TabIndicator({ tab }: { tab: Terminal }) {
  if (tab.kind === "terminal") return null;
  if (tab.running) {
    return (
      <Loader2Icon aria-label="Running" className="size-3 animate-spin text-muted-foreground" />
    );
  }
  if (tab.exit && tab.exit.code !== 0) {
    return <span role="img" aria-label="Failed" className="size-1.5 rounded-full bg-destructive" />;
  }
  return null;
}

function SetupEmpty({
  workspace,
  scripts,
}: {
  workspace: Workspace;
  scripts: ScriptsInfo | undefined;
}) {
  const run = useRunSetup();
  return (
    <div className="flex h-full flex-col items-center justify-center gap-2 p-6 text-center">
      {scripts?.setup ? (
        <>
          <p className="text-sm text-muted-foreground">
            The setup script has not run in this workspace.
          </p>
          <code className="max-w-full truncate rounded bg-muted px-1.5 py-0.5 font-mono text-xs">
            {scripts.setup}
          </code>
          <Button size="sm" onClick={() => run.mutate(workspace.id)} disabled={run.isPending}>
            Run setup
          </Button>
        </>
      ) : (
        <>
          <p className="text-sm text-muted-foreground">No setup script yet.</p>
          <p className="max-w-xs text-xs text-muted-foreground">
            A setup script installs dependencies when a workspace is created, for example{" "}
            <code className="font-mono">pnpm install</code>.
          </p>
          <Button size="sm" variant="outline" onClick={() => openSettings(workspace.project_id)}>
            <FileCogIcon />
            Open .circles/settings.toml
          </Button>
        </>
      )}
    </div>
  );
}

function RunEmpty({
  workspace,
  scripts,
}: {
  workspace: Workspace;
  scripts: ScriptsInfo | undefined;
}) {
  const run = useRunScript();
  const first = scripts?.run[0];
  if (!first) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-2 p-6 text-center">
        <p className="text-sm text-muted-foreground">No run script yet.</p>
        <p className="max-w-xs text-xs text-muted-foreground">
          A run script starts your app for testing, for example{" "}
          <code className="font-mono">pnpm dev --port $CIRCLES_PORT</code>.
        </p>
        <Button size="sm" variant="outline" onClick={() => openSettings(workspace.project_id)}>
          <FileCogIcon />
          Open .circles/settings.toml
        </Button>
      </div>
    );
  }
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 p-6 text-center">
      <PlayIcon className="size-10 text-muted-foreground/60" />
      <Button onClick={() => run.mutate({ workspaceId: workspace.id })} disabled={run.isPending}>
        Start {first.name}
        <kbd className="ml-1 text-xs text-primary-foreground/70">⌘R</kbd>
      </Button>
      <p className="text-xs text-muted-foreground">Test your changes here.</p>
      {scripts && scripts.run.length > 1 ? (
        <div className="flex flex-wrap justify-center gap-1.5">
          {scripts.run.slice(1).map((script) => (
            <Button
              key={script.name}
              size="xs"
              variant="outline"
              onClick={() => run.mutate({ workspaceId: workspace.id, name: script.name })}
            >
              <ScriptIcon name={script.icon} />
              {script.name}
            </Button>
          ))}
        </div>
      ) : null}
    </div>
  );
}

/** Run or stop, plus the menu of run scripts. */
function RunControls({
  workspace,
  tab,
  scripts,
}: {
  workspace: Workspace;
  tab: Terminal;
  scripts: ScriptsInfo | undefined;
}) {
  const run = useRunScript();
  const stop = useStopScript();
  const first = scripts?.run[0];
  if (!first) return null;

  return (
    <div className="flex items-center">
      {tab.running ? (
        <Button
          size="xs"
          variant="ghost"
          aria-label="Stop"
          disabled={stop.isPending}
          onClick={() => stop.mutate(workspace.id)}
          className="gap-1"
        >
          <SquareIcon className="fill-current" />
          Stop
        </Button>
      ) : (
        <Tooltip>
          <TooltipTrigger
            render={
              <Button
                size="xs"
                variant="ghost"
                aria-label={`Run ${first.name}`}
                disabled={run.isPending}
                onClick={() => run.mutate({ workspaceId: workspace.id })}
                className="gap-1"
              />
            }
          >
            <PlayIcon className="fill-current" />
            Run
          </TooltipTrigger>
          <TooltipContent>
            {first.name} <span className="ml-1 text-muted-foreground">⌘R</span>
          </TooltipContent>
        </Tooltip>
      )}
      <DropdownMenu>
        <DropdownMenuTrigger
          render={
            <Button
              size="icon-xs"
              variant="ghost"
              aria-label="Run scripts"
              className="text-muted-foreground"
            />
          }
        >
          <ChevronDownIcon />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-56">
          {scripts?.run.map((script) => (
            <DropdownMenuItem
              key={script.name}
              onClick={() => run.mutate({ workspaceId: workspace.id, name: script.name })}
            >
              <ScriptIcon name={script.icon} />
              <span className="truncate">{script.name}</span>
              {script.default ? (
                <span className="ml-auto text-2xs text-muted-foreground">default</span>
              ) : null}
            </DropdownMenuItem>
          ))}
          <DropdownMenuSeparator />
          <DropdownMenuItem onClick={() => openSettings(workspace.project_id)}>
            <FileCogIcon />
            Edit scripts
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}

/** Which tab is selected: the user's choice, else the free terminal, else the first tab. */
export function selectedTab(tabs: Terminal[], chosen: string | undefined): Terminal | undefined {
  return tabs.find((t) => t.id === chosen) ?? tabs.find((t) => t.kind === "terminal") ?? tabs[0];
}

/**
 * The terminal tabs of a workspace: Setup, Run, and free terminals. It is the panel at the bottom
 * of the center column, and the maximise button gives it the whole column.
 */
export function TerminalPanel({ workspace }: { workspace: Workspace }) {
  const terminals = useTerminals(workspace.id);
  const scripts = useScripts(workspace.id);
  const create = useCreateTerminal();
  const close = useDeleteTerminal();
  const setupScript = useRunSetup();

  const chosen = useUi((s) => s.dockTab[workspace.id]);
  const select = useUi((s) => s.selectDockTab);
  const dockOpen = useUi((s) => s.dockOpen);
  const toggleDock = useUi((s) => s.toggleDock);
  const bigTerminal = useUi((s) => s.bigTerminal);
  const toggleBig = useUi((s) => s.toggleBigTerminal);

  const tabs = terminals.data ?? [];
  const tab = selectedTab(tabs, chosen);

  // While the setup script runs, show it.
  // When the shell of a free terminal ends, because the user typed `exit`, the tab is cleaned up
  // for them: an extra terminal closes, and the last one gets a new, clean shell. A shell that
  // keeps ending at once is left alone, so it cannot restart forever.
  const [fresh, setFresh] = useState<Record<string, number>>({});
  const restarts = useRef<number[]>([]);
  const ended = tab?.kind === "terminal" && !tab.running && tab.exit !== null;
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs when the shell of the tab ends
  useEffect(() => {
    if (!tab || !ended || !dockOpen) return;
    if (tabs.filter((t) => t.kind === "terminal").length > 1) {
      close.mutate(tab.id);
      return;
    }
    const now = Date.now();
    restarts.current = restarts.current.filter((at) => now - at < 10_000);
    if (restarts.current.length >= 3) return;
    restarts.current.push(now);
    ipc
      .terminalRestart(tab.id)
      .then(() => setFresh((f) => ({ ...f, [tab.id]: (f[tab.id] ?? 0) + 1 })))
      .catch(() => {});
  }, [tab?.id, ended, dockOpen]);

  // Opening the terminal when there is no terminal makes one, so there is always a shell to type
  // in. Closing every terminal is how a person asks for a clean start.
  const makingOne = useRef(false);
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs when the terminal opens or its list changes
  useEffect(() => {
    if (!dockOpen || !terminals.isSuccess || makingOne.current) return;
    if (terminals.data.some((t) => t.kind === "terminal")) return;
    makingOne.current = true;
    create.mutate(workspace.id, {
      onSuccess: (created) => select(workspace.id, created.id),
      onSettled: () => {
        makingOne.current = false;
      },
    });
  }, [dockOpen, terminals.isSuccess, terminals.data, workspace.id]);

  const announced = useRef<string | null>(null);
  useEffect(() => {
    if (workspace.state !== "setup_running" || announced.current === workspace.id) return;
    const setup = tabs.find((t) => t.kind === "setup");
    if (!setup) return;
    announced.current = workspace.id;
    select(workspace.id, setup.id);
    useUi.getState().setDockOpen(true);
  }, [workspace.state, workspace.id, tabs, select]);

  const collapsed = !dockOpen;

  const addTerminal = () =>
    create.mutate(workspace.id, {
      onSuccess: (created) => {
        select(workspace.id, created.id);
        useUi.getState().setDockOpen(true);
      },
    });

  // Closed, the bottom panel shows nothing. The effects above still run, so a script that starts
  // opens it.
  if (collapsed) return null;

  return (
    <section aria-label="Terminal" className="flex h-full min-h-0 flex-col bg-background">
      <div className="relative z-30 flex h-9 shrink-0 items-center gap-1 border-b border-border px-1.5">
        <Button
          variant="ghost"
          size="icon-xs"
          aria-label="Hide terminal"
          onClick={toggleDock}
          className="text-muted-foreground"
        >
          <ChevronDownIcon />
        </Button>

        <div
          role="tablist"
          aria-label="Terminal tabs"
          className="flex min-w-0 flex-1 items-center gap-0.5 overflow-x-auto"
        >
          {tabs.map((t) => (
            <div
              key={t.id}
              className={cn(
                "group/tab flex h-7 shrink-0 items-center rounded-md text-xs",
                tab?.id === t.id
                  ? "bg-muted text-foreground"
                  : "text-muted-foreground hover:text-foreground",
              )}
            >
              <button
                type="button"
                role="tab"
                aria-selected={tab?.id === t.id}
                onClick={() => {
                  select(workspace.id, t.id);
                  useUi.getState().setDockOpen(true);
                }}
                className="flex h-7 items-center gap-1.5 rounded-md px-2 outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
              >
                {t.title}
                <TabIndicator tab={t} />
              </button>
              {t.kind === "terminal" ? (
                <button
                  type="button"
                  aria-label={`Close ${t.title}`}
                  onClick={() => close.mutate(t.id)}
                  className="mr-1 flex size-4 items-center justify-center rounded text-muted-foreground opacity-0 outline-none hover:text-foreground focus-visible:opacity-100 group-hover/tab:opacity-100"
                >
                  <XIcon className="size-3" />
                </button>
              ) : null}
            </div>
          ))}
          <Button
            variant="ghost"
            size="icon-xs"
            aria-label="New terminal"
            disabled={create.isPending}
            onClick={addTerminal}
            className="shrink-0 text-muted-foreground"
          >
            <PlusIcon />
          </Button>
        </div>

        <div className="flex shrink-0 items-center">
          {tab?.kind === "run" ? (
            <RunControls workspace={workspace} tab={tab} scripts={scripts.data} />
          ) : null}
          {tab?.kind === "setup" && scripts.data?.setup ? (
            <Button
              size="xs"
              variant="ghost"
              disabled={tab.running || setupScript.isPending}
              onClick={() => setupScript.mutate(workspace.id)}
            >
              Run again
            </Button>
          ) : null}
          <Button
            variant="ghost"
            size="icon-xs"
            aria-label={bigTerminal ? "Restore terminal size" : "Maximize terminal"}
            onClick={toggleBig}
            className="text-muted-foreground"
          >
            {bigTerminal ? <Minimize2Icon /> : <Maximize2Icon />}
          </Button>
        </div>
      </div>

      {
        <div className="min-h-0 flex-1 overflow-y-auto">
          {!tab ? null : tab.kind === "setup" && !tab.has_output && !tab.running ? (
            <SetupEmpty workspace={workspace} scripts={scripts.data} />
          ) : tab.kind === "run" && !tab.has_output && !tab.running ? (
            <RunEmpty workspace={workspace} scripts={scripts.data} />
          ) : (
            <TerminalView key={`${tab.id}:${fresh[tab.id] ?? 0}`} terminalId={tab.id} />
          )}
        </div>
      }
    </section>
  );
}
