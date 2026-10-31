import type { ActionResult } from "@circles/bindings/ActionResult";
import type { SetupItem } from "@circles/bindings/SetupItem";
import { Button } from "@circles/ui/components/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@circles/ui/components/dialog";
import { cn } from "@circles/ui/utils";
import { useQuery } from "@tanstack/react-query";
import {
  CheckCircle2Icon,
  CircleDashedIcon,
  CopyIcon,
  Loader2Icon,
  ShieldAlertIcon,
  XCircleIcon,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { useProjects } from "@/features/projects/api";
import { ipc } from "@/lib/ipc";
import { useUi } from "@/stores/ui";
import { useOnboarding } from "./store";

/** One row of the checklist: its state, why it is there, and the button that does the next step. */
function SetupRow({ item, onChanged }: { item: SetupItem; onChanged: () => void }) {
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<ActionResult | null>(null);
  const run = async () => {
    if (!item.action) return;
    setBusy(true);
    try {
      setResult(await ipc.setupAction(item.action.id));
      onChanged();
    } catch (e) {
      setResult({ ok: false, message: e instanceof Error ? e.message : String(e) });
    } finally {
      setBusy(false);
    }
  };
  const Icon =
    item.state === "ready"
      ? CheckCircle2Icon
      : item.state === "needs_you"
        ? XCircleIcon
        : CircleDashedIcon;
  const label =
    item.state === "ready"
      ? "Ready"
      : item.state === "needs_you"
        ? "Needs attention"
        : item.state === "unknown"
          ? "Not checked yet"
          : "Optional";
  return (
    <li className="flex items-start gap-3 py-2.5" data-testid={`setup-${item.id}`}>
      <Icon
        aria-label={label}
        className={cn(
          "mt-0.5 size-4 shrink-0",
          item.state === "ready" && "text-success",
          item.state === "needs_you" && "text-destructive",
          item.state !== "ready" && item.state !== "needs_you" && "text-muted-foreground",
        )}
      />
      <div className="min-w-0 flex-1">
        <p className="text-sm font-medium">
          {item.title}
          {item.state === "ready" && item.detail ? (
            <span className="ml-2 text-xs font-normal text-muted-foreground">{item.detail}</span>
          ) : null}
        </p>
        <p className="text-xs text-muted-foreground">{item.why}</p>
        {item.state !== "ready" && item.detail ? (
          <p className="mt-0.5 text-xs">{item.detail}</p>
        ) : null}
        {item.fix ? (
          <code className="mt-1 inline-block max-w-full break-all rounded bg-muted px-1.5 py-0.5 font-mono text-xs">
            {item.fix}
          </code>
        ) : null}
        {result ? (
          <p
            role="status"
            className={cn("mt-1 text-xs", result.ok ? "text-success" : "text-warning")}
          >
            {result.message}
          </p>
        ) : null}
      </div>
      {item.action ? (
        <Button size="sm" variant="outline" disabled={busy} onClick={() => void run()}>
          {busy ? <Loader2Icon className="animate-spin" /> : null}
          {item.action.label}
        </Button>
      ) : null}
    </li>
  );
}

/** Shown on the first launch: what Circles needs, what agents may do, and the first project. */
export function WelcomeDialog() {
  const done = useOnboarding((s) => s.done);
  const armed = useOnboarding((s) => s.armed);
  const finish = useOnboarding((s) => s.finish);
  const arm = useOnboarding((s) => s.arm);
  const setAddProjectOpen = useUi((s) => s.setAddProjectOpen);
  const [understood, setUnderstood] = useState(false);
  const projects = useProjects().data;
  // Someone who already has projects has been through this. Only the launch decides that, so
  // "show again" in the settings still shows it.
  const checked = useRef(false);
  useEffect(() => {
    if (projects === undefined || checked.current) return;
    checked.current = true;
    if (projects.length > 0) finish();
    else if (!done) arm();
  }, [projects, finish, arm, done]);
  const status = useQuery({
    queryKey: ["setup-status"],
    queryFn: ipc.setupStatus,
    enabled: !done && armed,
    staleTime: 0,
    // Coming back from Terminal or System Settings checks again.
    refetchOnWindowFocus: true,
  });
  const items = status.data ?? [];
  const required = items.filter((i) => i.required);
  const optional = items.filter((i) => !i.required);
  const blocking = required.filter((i) => i.state !== "ready");
  const ready = status.isSuccess && blocking.length === 0;

  const copyReport = async () => {
    try {
      await navigator.clipboard.writeText(await ipc.diagnosticsReport());
      toast("Diagnostics copied");
    } catch (e) {
      toast.error(e instanceof Error ? e.message : "Cannot copy the diagnostics");
    }
  };

  return (
    <Dialog open={!done && armed} onOpenChange={() => {}}>
      <DialogContent showCloseButton={false} className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>Welcome to Circles</DialogTitle>
          <DialogDescription>
            Run several coding agents side by side, each in its own copy of your repository.
            Everything stays on this Mac.
          </DialogDescription>
        </DialogHeader>

        <div className="flex max-h-[55vh] flex-col gap-4 overflow-y-auto pr-1">
          <section aria-label="What Circles needs">
            <div className="flex items-center justify-between">
              <h3 className="text-sm font-medium">What Circles needs</h3>
              <Button
                variant="ghost"
                size="sm"
                disabled={status.isFetching}
                onClick={() => void status.refetch()}
              >
                {status.isFetching ? <Loader2Icon className="animate-spin" /> : null}
                Check again
              </Button>
            </div>
            <ul className="divide-y divide-border">
              {required.map((item) => (
                <SetupRow key={item.id} item={item} onChanged={() => void status.refetch()} />
              ))}
            </ul>
          </section>

          <section aria-label="Nice to have">
            <h3 className="text-sm font-medium">Nice to have</h3>
            <p className="text-xs text-muted-foreground">
              None of this is needed. You can do it later in Settings, Setup.
            </p>
            <ul className="divide-y divide-border">
              {optional.map((item) => (
                <SetupRow key={item.id} item={item} onChanged={() => void status.refetch()} />
              ))}
            </ul>
          </section>
        </div>

        <section
          aria-label="Security notice"
          className="flex gap-3 rounded-lg border border-warning/50 bg-warning/10 p-3 text-sm"
        >
          <ShieldAlertIcon className="mt-0.5 size-4 shrink-0 text-warning" />
          <div className="flex flex-col gap-2">
            <p>
              Agents run with your user's permissions. They can read and change files and run
              commands on this Mac. Each works in its own workspace folder, and Circles asks before
              risky steps unless you change that in Settings. Full access is off by default.
            </p>
            <label className="flex items-center gap-2 text-sm font-medium">
              <input
                type="checkbox"
                checked={understood}
                onChange={(e) => setUnderstood(e.target.checked)}
                className="size-4 accent-foreground"
              />
              I understand
            </label>
          </div>
        </section>

        <DialogFooter className="sm:justify-between">
          <Button variant="ghost" size="sm" onClick={() => void copyReport()}>
            <CopyIcon />
            Copy diagnostics
          </Button>
          <div className="flex flex-wrap items-center justify-end gap-2">
            {status.isSuccess && !ready ? (
              <span className="text-xs text-muted-foreground">
                {blocking.length === 1
                  ? `${blocking[0].title} is still needed.`
                  : `${blocking.length} things are still needed.`}
              </span>
            ) : null}
            <Button variant="outline" disabled={!understood} onClick={finish}>
              Skip for now
            </Button>
            <Button
              disabled={!understood || !ready}
              onClick={() => {
                finish();
                setAddProjectOpen(true);
              }}
            >
              Add your first project
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
