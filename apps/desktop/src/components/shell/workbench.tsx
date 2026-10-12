import { cn } from "@circles/ui/utils";
import { useEffect, useRef, useState } from "react";
import { Group, Panel, Separator, usePanelRef } from "react-resizable-panels";
import { Outlet, useMatch } from "react-router";
import { TerminalPanel } from "@/features/terminal/terminal-panel";
import { useWorkspace } from "@/features/workspaces/api";
import { usePanelSync } from "@/hooks/use-panel-sync";
import { useUserResizing } from "@/hooks/use-user-resizing";
import { DOCK, useUi } from "@/stores/ui";

const HANDLE = cn(
  "group/handle relative h-1.5 shrink-0 outline-none",
  "before:absolute before:inset-x-0 before:top-1/2 before:h-px before:-translate-y-1/2 before:bg-border before:transition-colors",
  "data-[separator=hover]:before:bg-ring data-[separator=active]:before:bg-ring focus-visible:before:bg-ring",
);

/** How tall the terminal opens, in pixels: what the user dragged it to, else 30% of the window. */
export function openHeight(): number {
  const { dockHeight } = useUi.getState();
  const wanted = dockHeight ?? Math.round(window.innerHeight * DOCK.share);
  // Leave room for the page above it.
  const room = Math.max(DOCK.min, window.innerHeight - 200);
  return Math.min(Math.max(wanted, DOCK.min), Math.min(DOCK.max, room));
}

/** The panel's height now, in pixels, whichever shape the library reports it in. */
function heightOf(panel: { getSize: () => unknown }): number {
  const size = panel.getSize() as number | { inPixels: number };
  return typeof size === "number" ? size : size.inPixels;
}

/**
 * The page of the center column, with the terminal under it in a workspace, the way an editor has
 * its panel. The terminal sits inside the center column, so it never runs under the side panes.
 */
export function Workbench() {
  const workspaceId = useMatch("/workspace/:id")?.params.id;
  const workspace = useWorkspace(workspaceId).data;
  const usable = !!workspace && workspace.state !== "archived" && workspace.state !== "errored";
  const dockOpen = useUi((s) => s.dockOpen);
  const setDockOpen = useUi((s) => s.setDockOpen);
  const setDockHeight = useUi((s) => s.setDockHeight);
  const maximized = useUi((s) => s.bigTerminal);
  const page = usePanelRef();
  const resizing = useUserResizing();
  // The starting size is read once. The panel library owns the size while it is dragged.
  const [initialDock] = useState(() => (useUi.getState().dockOpen ? openHeight() : DOCK.collapsed));
  const dock = usePanelSync({
    visible: dockOpen,
    canShow: true,
    setOpen: setDockOpen,
    setWidth: setDockHeight,
    collapsedPx: DOCK.collapsed,
    remember: resizing.isResizing,
  });

  // Every way of opening the terminal, the button, the shortcut or a script starting, ends here,
  // so they all give the same height. A panel that started collapsed has no earlier size to go
  // back to, and the library may settle on its own size after this runs, so the height is set
  // again until it holds.
  const wasOpen = useRef(dockOpen);
  useEffect(() => {
    const panel = dock.ref.current;
    const opening = dockOpen && !wasOpen.current;
    wasOpen.current = dockOpen;
    if (!opening || !panel) return;
    if (panel.isCollapsed()) panel.expand();
    const target = openHeight();
    panel.resize(`${target}px`);
    let tries = 0;
    let frame = 0;
    const settle = () => {
      const current = dock.ref.current;
      if (!current || !useUi.getState().dockOpen || useUi.getState().bigTerminal) return;
      if (Math.abs(heightOf(current) - target) > 8 && tries < 5) {
        tries += 1;
        current.resize(`${target}px`);
        frame = requestAnimationFrame(settle);
      }
    };
    frame = requestAnimationFrame(settle);
    return () => cancelAnimationFrame(frame);
  }, [dockOpen, dock.ref]);

  // Maximised, the terminal takes the whole column. The page is only collapsed, not removed, so
  // the chat keeps running and comes back as it was.
  const squeezePage = maximized && dockOpen && usable;
  useEffect(() => {
    const panel = page.current;
    if (!panel) return;
    if (squeezePage && !panel.isCollapsed()) panel.collapse();
    if (!squeezePage && panel.isCollapsed()) panel.expand();
  }, [squeezePage, page]);

  // The page stays in the same place whether or not there is a terminal, so going from the home
  // page to a workspace, or a workspace finishing loading, does not mount the page again.
  return (
    <Group orientation="vertical" className="min-h-0 flex-1">
      <Panel id="page" panelRef={page} collapsible collapsedSize={0} minSize={120}>
        <main className="h-full overflow-y-auto">
          <Outlet />
        </main>
      </Panel>
      {usable && workspace ? (
        <>
          <Separator
            {...resizing.handlers}
            disabled={!dockOpen || maximized}
            className={cn(HANDLE, (!dockOpen || maximized) && "pointer-events-none h-0 opacity-0")}
          />
          <Panel
            id="dock"
            panelRef={dock.ref}
            collapsible
            collapsedSize={DOCK.collapsed}
            minSize={DOCK.min}
            maxSize={DOCK.max}
            defaultSize={initialDock}
            groupResizeBehavior="preserve-pixel-size"
            onResize={dock.onResize}
          >
            <TerminalPanel workspace={workspace} />
          </Panel>
        </>
      ) : null}
    </Group>
  );
}
