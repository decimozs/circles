import { ShellControls } from "@/components/shell/shell-controls";
import { useUi } from "@/stores/ui";

/**
 * Overlay title bar. The strip is a window drag region. The control cluster (sidebar toggle,
 * back, forward) sits at the right edge of the sidebar header, on the same line as the native
 * traffic lights. While the sidebar is hidden it moves here, beside the traffic lights.
 */
export function TitleBar() {
  const leftVisible = useUi((s) => s.leftOpen && !s.zen);

  return (
    <>
      <div data-tauri-drag-region className="fixed inset-x-0 top-0 z-20 h-10" />
      {leftVisible ? null : (
        <div className="fixed top-0 left-0 z-30 flex h-10 items-center pl-[84px]">
          <ShellControls />
        </div>
      )}
    </>
  );
}
