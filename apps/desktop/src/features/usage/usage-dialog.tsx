import { Dialog, DialogContent, DialogTitle } from "@circles/ui/components/dialog";
import { cn } from "@circles/ui/utils";
import { useMutation } from "@tanstack/react-query";
import { Loader2Icon, RefreshCwIcon } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";
import { IconButton } from "@/components/shell/icon-button";
import { describeError } from "@/features/chat/use-session";
import { ipc } from "@/lib/ipc";
import { formatAge } from "@/lib/time";
import { useUi } from "@/stores/ui";
import { useUsage, useUsageSync } from "./api";
import { folderName, RANGES, type RangeId } from "./format";
import { LimitsView } from "./limits-view";
import { StatsView } from "./stats-view";

type View = "cost" | "tokens" | "limits";
const VIEWS: { id: View; label: string }[] = [
  { id: "cost", label: "Cost" },
  { id: "tokens", label: "Tokens" },
  { id: "limits", label: "Limits" },
];

const REMEMBER = "circles.usage.range";

function savedRange(): RangeId {
  try {
    const saved = localStorage.getItem(REMEMBER);
    if (RANGES.some((r) => r.id === saved)) return saved as RangeId;
  } catch {
    // Storage can be blocked. The default is fine.
  }
  return "30d";
}

const SELECT =
  "h-7 max-w-52 truncate rounded-md bg-transparent pr-1 text-sm outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/60";

function Segmented<T extends string>({
  label,
  options,
  value,
  onChange,
}: {
  label: string;
  options: { id: T; label: string }[];
  value: T;
  onChange: (id: T) => void;
}) {
  return (
    <fieldset className="flex rounded-md bg-muted p-0.5 text-xs">
      <legend className="sr-only">{label}</legend>
      {options.map((o) => (
        <button
          key={o.id}
          type="button"
          aria-pressed={value === o.id}
          onClick={() => onChange(o.id)}
          className={cn(
            "rounded px-2.5 py-1 whitespace-nowrap",
            value === o.id ? "bg-background text-foreground shadow-sm" : "text-muted-foreground",
          )}
        >
          {o.label}
        </button>
      ))}
    </fieldset>
  );
}

/** When the numbers were last brought up to date, and what the background read is doing. */
function StatusLine({
  refreshedAt,
  refreshing,
  done,
  total,
}: {
  refreshedAt: number | null;
  refreshing: boolean;
  done: number;
  total: number;
}) {
  const age =
    refreshedAt === null
      ? "Not read yet"
      : Date.now() - refreshedAt < 60_000
        ? "Updated just now"
        : `Updated ${formatAge(refreshedAt)} ago`;
  return (
    <span role="status" className="flex items-center gap-1.5">
      {refreshing ? <Loader2Icon className="size-3 animate-spin" /> : null}
      {age}
      {refreshing ? (
        <span>{total > 0 ? ` · reading ${done} of ${total} files` : " · reading…"}</span>
      ) : null}
    </span>
  );
}

/** Tokens and what they would cost, from Claude Code's own history on this Mac. What was read is
 * kept, so this opens at once and brings itself up to date in the background. */
export function UsageDialog() {
  const open = useUi((s) => s.usageOpen);
  const setOpen = useUi((s) => s.setUsageOpen);
  const [range, setRange] = useState<RangeId>(savedRange);
  const [view, setView] = useState<View>("cost");
  const [folder, setFolder] = useState<string | null>(null);
  const [confirmRebuild, setConfirmRebuild] = useState(false);
  const usage = useUsage(range, folder, open);
  useUsageSync();
  // Opening the dialog starts a read of what is new. It shows what is kept in the meantime.
  useEffect(() => {
    if (open) ipc.usageRefresh().catch(() => {});
  }, [open]);
  const rebuild = useMutation({
    mutationFn: ipc.usageRebuild,
    onSuccess: () => setConfirmRebuild(false),
    onError: (e) => toast.error(describeError(e)),
  });
  const data = usage.data;
  const folders = data?.folders ?? [];

  const pickRange = (id: RangeId) => {
    setRange(id);
    try {
      localStorage.setItem(REMEMBER, id);
    } catch {
      // Not remembered. It still applies now.
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        setOpen(next);
        if (!next) setConfirmRebuild(false);
      }}
    >
      <DialogContent
        showCloseButton
        className="flex max-h-[88vh] flex-col gap-0 overflow-hidden p-0 sm:max-w-5xl"
      >
        <header className="flex flex-wrap items-center gap-x-3 gap-y-2 border-b border-border py-3 pr-12 pl-6">
          <DialogTitle className="text-sm font-normal text-muted-foreground">Usage</DialogTitle>
          <span className="text-muted-foreground/60">/</span>
          <select
            aria-label="Environment"
            className={SELECT}
            value={folder ?? ""}
            onChange={(e) => setFolder(e.target.value || null)}
          >
            <option value="">All environments</option>
            {folders.map((f) => (
              <option key={f} value={f}>
                {folderName(f)}
              </option>
            ))}
          </select>
          <span className="text-muted-foreground/60">/</span>
          <select aria-label="Provider" className={SELECT} defaultValue="claude">
            <option value="claude">Claude Code</option>
          </select>
          <div className="ml-auto flex flex-wrap items-center gap-2">
            {view !== "limits" ? (
              <Segmented label="Range" options={RANGES} value={range} onChange={pickRange} />
            ) : null}
            <Segmented label="View" options={VIEWS} value={view} onChange={setView} />
            <IconButton label="Refresh" onClick={() => ipc.usageRefresh().catch(() => {})}>
              <RefreshCwIcon className={cn(data?.refreshing && "animate-spin")} />
            </IconButton>
          </div>
        </header>

        <div className="min-h-0 flex-1 overflow-y-auto">
          {view === "limits" ? (
            <LimitsView enabled={open} />
          ) : usage.isPending ? (
            <p
              role="status"
              className="flex items-center gap-2 px-6 py-8 text-sm text-muted-foreground"
            >
              <Loader2Icon className="size-4 animate-spin" />
              Loading…
            </p>
          ) : usage.isError ? (
            <p role="alert" className="px-6 py-8 text-sm text-destructive">
              {describeError(usage.error)}
            </p>
          ) : data && data.messages === 0 && (data.refreshing || data.refreshed_at === null) ? (
            <p
              role="status"
              className="flex items-center gap-2 px-6 py-8 text-sm text-muted-foreground"
            >
              <Loader2Icon className="size-4 animate-spin" />
              Reading Claude Code's history for the first time
              {data.files_total > 0 ? `: ${data.files_done} of ${data.files_total} files` : "…"}
            </p>
          ) : data && data.messages === 0 ? (
            <div className="flex flex-col gap-1 px-6 py-8 text-sm">
              <p>No Claude Code usage found in this range.</p>
              <p className="text-muted-foreground">
                {data.folders.length > 0
                  ? `Looked in ${data.folders.map(folderName).join(", ")}.`
                  : "No Claude folder with history was found on this Mac."}
              </p>
            </div>
          ) : data ? (
            <StatsView metric={view} range={range} summary={data} />
          ) : null}
        </div>

        {view !== "limits" && data ? (
          <footer className="flex flex-wrap items-center gap-x-4 gap-y-1 border-t border-border px-6 py-2 text-xs text-muted-foreground">
            <StatusLine
              refreshedAt={data.refreshed_at}
              refreshing={data.refreshing}
              done={data.files_done}
              total={data.files_total}
            />
            <span className="ml-auto flex items-center gap-2">
              {confirmRebuild ? (
                <>
                  <span>
                    Forget everything kept and read again? Usage of conversations Claude Code has
                    deleted cannot come back.
                  </span>
                  <button
                    type="button"
                    className="text-destructive hover:underline"
                    disabled={rebuild.isPending}
                    onClick={() => rebuild.mutate()}
                  >
                    Rebuild
                  </button>
                  <button
                    type="button"
                    className="hover:text-foreground"
                    onClick={() => setConfirmRebuild(false)}
                  >
                    Cancel
                  </button>
                </>
              ) : (
                <button
                  type="button"
                  className="hover:text-foreground"
                  onClick={() => setConfirmRebuild(true)}
                >
                  Rebuild history
                </button>
              )}
            </span>
          </footer>
        ) : null}
      </DialogContent>
    </Dialog>
  );
}
