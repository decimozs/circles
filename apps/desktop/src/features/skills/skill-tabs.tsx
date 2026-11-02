import { cn } from "@amplify/ui/utils";
import { PlusIcon, SparklesIcon, XIcon } from "lucide-react";
import { useSkillsUi } from "./store";

/** The skills opened beside the list, as tabs in the top bar. The plus goes back to the list to
 * pick another one. */
export function SkillTabs() {
  const tabs = useSkillsUi((s) => s.tabs);
  const selected = useSkillsUi((s) => s.selected);
  const selectTab = useSkillsUi((s) => s.selectTab);
  const closeTab = useSkillsUi((s) => s.closeTab);
  return (
    <div
      role="tablist"
      aria-label="Open skills"
      className="flex h-full min-w-0 items-center gap-1 overflow-x-auto"
    >
      {tabs.map((t) => (
        <span
          key={t.key}
          title={t.path}
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
            className="flex h-full max-w-40 items-center gap-1.5 rounded-md pr-1 pl-2.5 outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
          >
            <SparklesIcon className="size-3.5 shrink-0" />
            <span className="truncate">{t.name}</span>
          </button>
          <button
            type="button"
            aria-label={`Close ${t.name}`}
            onClick={() => closeTab(t.key)}
            className="mr-1 rounded p-0.5 hover:bg-background"
          >
            <XIcon className="size-3" />
          </button>
        </span>
      ))}
      <button
        type="button"
        aria-label="Open another skill"
        onClick={() => selectTab(null)}
        className="flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground"
      >
        <PlusIcon className="size-3.5" />
      </button>
    </div>
  );
}
