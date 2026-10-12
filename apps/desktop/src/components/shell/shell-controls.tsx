import { ArrowLeftIcon, ArrowRightIcon, PanelLeftIcon } from "lucide-react";
import { IconButton } from "@/components/shell/icon-button";
import { useHistoryNav } from "@/hooks/use-history-nav";
import { useUi } from "@/stores/ui";

/** Sidebar toggle plus back and forward, shown as one cluster. */
export function ShellControls() {
  const toggleLeft = useUi((s) => s.toggleLeft);
  const { canGoBack, canGoForward, back, forward } = useHistoryNav();

  return (
    <div className="flex items-center gap-0.5">
      <IconButton label="Toggle sidebar" hint="⌘B" onClick={toggleLeft}>
        <PanelLeftIcon />
      </IconButton>
      <IconButton label="Back" hint="⌘[" onClick={back} disabled={!canGoBack}>
        <ArrowLeftIcon />
      </IconButton>
      <IconButton label="Forward" hint="⌘]" onClick={forward} disabled={!canGoForward}>
        <ArrowRightIcon />
      </IconButton>
    </div>
  );
}
