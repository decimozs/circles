import { cn } from "@circles/ui/utils";
import { GitPullRequestIcon, PlusIcon, XIcon } from "lucide-react";
import { usePrs } from "./store";

/** The pull requests opened beside the list, as tabs in the top bar. The plus goes back to the list
 * to pick another one. */
export function PrTabs() {
  const tabs = usePrs((s) => s.tabs);
  const selected = usePrs((s) => s.selected);
  const selectTab = usePrs((s) => s.selectTab);
  const closeTab = usePrs((s) => s.closeTab);
  return (
    <div
      role="tablist"
      aria-label="Open pull requests"
      className="flex h-full min-w-0 items-center gap-1 overflow-x-auto"
    >
      {tabs.map((t) => (
        <span
          key={t.key}
          title={t.title}
          className={cn(
            "flex h-7 shrink-0 items-center rounded-md text-xs",
            t.key === selected
              ? "bg-muted text-foreground"
              : "text-muted-foreground hover:bg-muted/50 hover:text-foreground",
          )}
        >
          <button
            type="button"
            role="tab"
            aria-selected={t.key === selected}
            onClick={() => selectTab(t.key)}
            className="flex h-full items-center gap-1.5 rounded-md pr-1 pl-2.5 outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
          >
            <GitPullRequestIcon className="size-3.5" />#{t.number}
          </button>
          <button
            type="button"
            aria-label={`Close #${t.number}`}
            onClick={() => closeTab(t.key)}
            className="mr-1 rounded p-0.5 hover:bg-background"
          >
            <XIcon className="size-3" />
          </button>
        </span>
      ))}
      <button
        type="button"
        aria-label="Open another pull request"
        onClick={() => selectTab(null)}
        className="flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground"
      >
        <PlusIcon className="size-3.5" />
      </button>
    </div>
  );
}
