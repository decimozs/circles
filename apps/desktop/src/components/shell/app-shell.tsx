import { cn } from "@circles/ui/utils";
import { useCallback, useRef } from "react";
import { Group, Panel, Separator } from "react-resizable-panels";
import { useMatch } from "react-router";
import { RightPanel } from "@/components/shell/right-panel";
import { ShellActions } from "@/components/shell/shell-actions";
import { ShortcutsDialog } from "@/components/shell/shortcuts-dialog";
import { TitleBar } from "@/components/shell/title-bar";
import { TopBar } from "@/components/shell/top-bar";
import { Workbench } from "@/components/shell/workbench";
import { useSessionSync, useSideSync } from "@/features/chat/api";
import { useComposerSync } from "@/features/composers/api";
import { ComposerDialog } from "@/features/composers/composer-dialog";
import { StopAllDialog } from "@/features/composers/stop-all";
import { CreateDialog } from "@/features/create/create-dialog";
import { useGenuiSync } from "@/features/genui/api";
import { WelcomeDialog } from "@/features/onboarding/welcome-dialog";
import { Palette } from "@/features/palette/palette";
import { CommitDialog } from "@/features/pr/commit-dialog";
import { PrDialog } from "@/features/pr/pr-dialog";
import { AddProjectDialog } from "@/features/projects/add-project-dialog";
import { useApplySettings } from "@/features/settings/apply";
import { SettingsNav } from "@/features/settings/settings-nav";
import { Sidebar } from "@/features/sidebar/sidebar";
import { useTerminalSync } from "@/features/terminal/api";
import { QuitDialog } from "@/features/terminal/quit-dialog";
import { UsageDialog } from "@/features/usage/usage-dialog";
import { useWorkspaceStateSync } from "@/features/workspaces/api";
import { CreateSectionDialog } from "@/features/workspaces/create-section-dialog";
import { useGlobalShortcuts } from "@/hooks/use-global-shortcuts";
import { usePanelSync } from "@/hooks/use-panel-sync";
import { useUserResizing } from "@/hooks/use-user-resizing";
import { RIGHT_PANEL, SIDEBAR, useUi } from "@/stores/ui";

const SEPARATOR =
  "w-px bg-border outline-none transition-colors data-[separator=hover]:bg-ring data-[separator=active]:bg-ring focus-visible:bg-ring";

export function AppShell() {
  useGlobalShortcuts();
  useApplySettings();
  useComposerSync();
  useWorkspaceStateSync();
  useTerminalSync();
  const openId = useMatch("/workspace/:id")?.params.id;
  const openRef = useRef(openId);
  openRef.current = openId;
  useSessionSync(useCallback(() => openRef.current, []));
  useSideSync();
  useGenuiSync();

  const { leftOpen, rightOpen, zen, leftWidth, rightWidth } = useUi.getState();
  const setLeftOpen = useUi((s) => s.setLeftOpen);
  const setRightOpen = useUi((s) => s.setRightOpen);
  const setLeftWidth = useUi((s) => s.setLeftWidth);
  const setRightWidth = useUi((s) => s.setRightWidth);
  const leftOpenNow = useUi((s) => s.leftOpen);
  const rightOpenNow = useUi((s) => s.rightOpen);
  const zenNow = useUi((s) => s.zen);
  const onWorkspace = useMatch("/workspace/:id") !== null;
  const onSettings = useMatch("/settings/*") !== null;
  const onComposer = useMatch("/composer/:id/*") !== null;

  const leftVisible = leftOpenNow && !zenNow;
  const rightVisible = rightOpenNow && !zenNow && (onWorkspace || onComposer);

  const resizing = useUserResizing();
  const left = usePanelSync({
    visible: leftVisible,
    canShow: !zenNow,
    setOpen: setLeftOpen,
    setWidth: setLeftWidth,
    remember: resizing.isResizing,
  });
  const right = usePanelSync({
    visible: rightVisible,
    canShow: !zenNow && (onWorkspace || onComposer),
    setOpen: setRightOpen,
    setWidth: setRightWidth,
    remember: resizing.isResizing,
  });

  return (
    <div className="h-full w-full overflow-clip bg-background text-foreground">
      <ShellActions />
      <ShortcutsDialog />
      <UsageDialog />
      <AddProjectDialog />
      <CreateDialog />
      <Palette />
      <CreateSectionDialog />
      <CommitDialog />
      <PrDialog />
      <QuitDialog />
      <ComposerDialog />
      <StopAllDialog />
      <WelcomeDialog />
      <TitleBar />
      <Group orientation="horizontal" className="h-full">
        <Panel
          id="left"
          panelRef={left.ref}
          collapsible
          collapsedSize={0}
          minSize={SIDEBAR.min}
          maxSize={SIDEBAR.max}
          defaultSize={leftOpen && !zen ? leftWidth : 0}
          groupResizeBehavior="preserve-pixel-size"
          onResize={left.onResize}
          className="border-r border-sidebar-border"
        >
          {onSettings ? <SettingsNav /> : <Sidebar />}
        </Panel>
        <Separator
          {...resizing.handlers}
          disabled={!leftVisible}
          className={cn(SEPARATOR, !leftVisible && "pointer-events-none opacity-0")}
        />
        <Panel id="center" minSize={320}>
          <div className="flex h-full min-w-0 flex-col">
            <TopBar leftVisible={leftVisible} />
            <Workbench />
          </div>
        </Panel>
        <Separator
          {...resizing.handlers}
          disabled={!rightVisible}
          className={cn(SEPARATOR, !rightVisible && "pointer-events-none opacity-0")}
        />
        <Panel
          id="right"
          panelRef={right.ref}
          collapsible
          collapsedSize={0}
          minSize={RIGHT_PANEL.min}
          maxSize={RIGHT_PANEL.max}
          defaultSize={rightOpen && !zen && (onWorkspace || onComposer) ? rightWidth : 0}
          groupResizeBehavior="preserve-pixel-size"
          onResize={right.onResize}
          className="border-l border-border"
        >
          <RightPanel />
        </Panel>
      </Group>
    </div>
  );
}
