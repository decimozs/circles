import { PanelRightIcon, SquareTerminalIcon } from "lucide-react";
import { useState } from "react";
import { useMatch, useNavigate } from "react-router";
import { ComposerOrb } from "@/components/composer-orb";
import { IconButton } from "@/components/shell/icon-button";
import { ChatHistory, ChatTabs } from "@/features/chat/chat-tabs";
import { useComposer, useWorkspaceLinks } from "@/features/composers/api";
import { useProjects } from "@/features/projects/api";
import { PrTabs } from "@/features/prs/pr-tabs";
import { usePrs } from "@/features/prs/store";
import { OpenFileMenu, useSettingsRoute } from "@/features/settings/settings-page";
import { useWorkspace } from "@/features/workspaces/api";
import { StatusIndicator } from "@/features/workspaces/status";
import { WorkspaceActions } from "@/features/workspaces/workspace-actions";
import { WorkspaceDetailsDialog, WorkspaceMenuItems } from "@/features/workspaces/workspace-info";
import { useUi } from "@/stores/ui";

/** Center column header. Left padding clears the title bar controls when the sidebar is hidden. */
export function TopBar({ leftVisible }: { leftVisible: boolean }) {
  const navigate = useNavigate();
  const [detailsOpen, setDetailsOpen] = useState(false);
  const match = useMatch("/workspace/:id");
  const composerMatch = useMatch("/composer/:id/*");
  const composer = useComposer(composerMatch?.params.id).data;
  const links = useWorkspaceLinks();
  const toggleRight = useUi((s) => s.toggleRight);
  const toggleDock = useUi((s) => s.toggleDock);
  const dockOpen = useUi((s) => s.dockOpen);
  const workspace = useWorkspace(match?.params.id).data;
  const onSettings = useMatch("/settings/*") !== null;
  const onPrs = useMatch("/pull-requests") !== null;
  const hasPrTabs = usePrs((s) => s.tabs.length > 0);
  const settingsRoute = useSettingsRoute();
  const project = useProjects().data?.find((p) => p.id === workspace?.project_id);

  return (
    <header
      className="pointer-events-none relative z-30 flex h-10 shrink-0 items-center pr-3 transition-[padding] duration-150"
      style={{ paddingLeft: leftVisible ? 16 : 176 }}
    >
      {composer ? (
        <>
          <div className="pointer-events-auto flex shrink-0 items-center gap-2">
            <ComposerOrb seed={composer.id} className="size-4" />
            <h1 className="text-sm font-semibold">{composer.name}</h1>
          </div>
          <div className="pointer-events-auto ml-3 min-w-0 flex-1 self-stretch">
            <ChatTabs workspaceId={composer.workspace_id} single />
          </div>
          <div className="pointer-events-auto flex shrink-0 items-center gap-1">
            {/* A Composer has one chat, so there are no closed chats to bring back. */}
            <IconButton label="Toggle right panel" hint="⌘⌥B" onClick={toggleRight}>
              <PanelRightIcon />
            </IconButton>
          </div>
        </>
      ) : null}
      {workspace ? (
        <>
          <div className="pointer-events-auto flex shrink-0 items-center gap-2">
            <StatusIndicator state={workspace.state} spinner={false} />
            <h1 className="text-sm font-semibold">{workspace.title ?? workspace.city}</h1>
            {links.get(workspace.id) ? (
              <button
                type="button"
                title="Open the run that started this workspace"
                onClick={() => {
                  const link = links.get(workspace.id);
                  if (link) navigate(`/composer/${link.composer_id}/run/${link.session_id}`);
                }}
                className="flex items-center gap-1 rounded-md bg-muted px-1.5 py-0.5 text-2xs text-muted-foreground hover:text-foreground"
              >
                <ComposerOrb seed={links.get(workspace.id)?.composer_id ?? ""} className="size-3" />
                Started by {links.get(workspace.id)?.composer_name}
              </button>
            ) : null}
          </div>
          <div className="pointer-events-auto ml-3 min-w-0 flex-1 self-stretch">
            <ChatTabs workspaceId={workspace.id} />
          </div>
          <div className="pointer-events-auto flex shrink-0 items-center gap-1">
            <ChatHistory workspaceId={workspace.id} />
            <IconButton
              label="Toggle terminal"
              hint="⌘J"
              aria-pressed={dockOpen}
              onClick={toggleDock}
              className={dockOpen ? "text-foreground" : undefined}
            >
              <SquareTerminalIcon />
            </IconButton>
            <WorkspaceActions
              workspace={workspace}
              onDeleted={() => navigate("/")}
              extra={
                <WorkspaceMenuItems
                  workspace={workspace}
                  onShowDetails={() => setDetailsOpen(true)}
                />
              }
            />
            <IconButton label="Toggle right panel" hint="⌘⌥B" onClick={toggleRight}>
              <PanelRightIcon />
            </IconButton>
          </div>
        </>
      ) : null}
      {onPrs && hasPrTabs ? (
        <div className="pointer-events-auto min-w-0 flex-1 self-stretch">
          <PrTabs />
        </div>
      ) : null}
      {onSettings ? (
        // The settings file is opened from the top bar, at the right, whichever page is shown.
        <div className="pointer-events-auto mt-2 ml-auto flex shrink-0 items-center">
          <OpenFileMenu projectId={settingsRoute.repoId} />
        </div>
      ) : null}
      {workspace ? (
        <WorkspaceDetailsDialog
          workspace={workspace}
          project={project}
          open={detailsOpen}
          onOpenChange={setDetailsOpen}
        />
      ) : null}
    </header>
  );
}
