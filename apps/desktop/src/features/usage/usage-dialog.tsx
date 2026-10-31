import { Dialog, DialogContent, DialogTitle } from "@circles/ui/components/dialog";
import { cn } from "@circles/ui/utils";
import { useQueryClient } from "@tanstack/react-query";
import { Loader2Icon, RefreshCwIcon } from "lucide-react";
import { useState } from "react";
import { IconButton } from "@/components/shell/icon-button";
import { describeError } from "@/features/chat/use-session";
import { useUi } from "@/stores/ui";
import { useUsage } from "./api";
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

/** Tokens and what they would cost, from Claude Code's own history on this Mac. */
export function UsageDialog() {
  const open = useUi((s) => s.usageOpen);
  const setOpen = useUi((s) => s.setUsageOpen);
  const queryClient = useQueryClient();
  const [range, setRange] = useState<RangeId>(savedRange);
  const [view, setView] = useState<View>("cost");
  const [folder, setFolder] = useState<string | null>(null);
  const usage = useUsage(range, folder, open);
  // The folders to choose from come from the unfiltered answer.
  const all = useUsage(range, null, open);
  const folders = all.data?.folders ?? [];

  const pickRange = (id: RangeId) => {
    setRange(id);
    try {
      localStorage.setItem(REMEMBER, id);
    } catch {
      // Not remembered. It still applies now.
    }
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
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
            <IconButton
              label="Refresh"
              onClick={() => queryClient.invalidateQueries({ queryKey: ["usage"] })}
            >
              <RefreshCwIcon className={cn(usage.isFetching && "animate-spin")} />
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
              Reading Claude Code's history. The first time takes a few seconds.
            </p>
          ) : usage.isError ? (
            <p role="alert" className="px-6 py-8 text-sm text-destructive">
              {describeError(usage.error)}
            </p>
          ) : usage.data.messages === 0 ? (
            <div className="flex flex-col gap-1 px-6 py-8 text-sm">
              <p>No Claude Code usage found in this range.</p>
              <p className="text-muted-foreground">
                {usage.data.folders.length > 0
                  ? `Looked in ${usage.data.folders.map(folderName).join(", ")}.`
                  : "No Claude folder with history was found on this Mac."}
              </p>
            </div>
          ) : (
            <StatsView metric={view} range={range} summary={usage.data} />
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
