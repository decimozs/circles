import { useMatch } from "react-router";
import { ChangesPanel } from "@/features/changes/changes-panel";
import { ChecksPanel } from "@/features/checks/checks-panel";
import { useComposer } from "@/features/composers/api";
import { ComposerPanel } from "@/features/composers/composer-panel";
import { useChanges } from "@/features/files/api";
import { FileTree } from "@/features/files/file-tree";
import { GraphPanel } from "@/features/graph/graph-panel";
import { NextActionBar } from "@/features/pr/next-action";
import { useWorkspace } from "@/features/workspaces/api";
import { useChat } from "@/stores/chat";
import { type RightTab, useReview } from "@/stores/review";

const TABS: { id: RightTab; label: string }[] = [
  { id: "files", label: "All files" },
  { id: "changes", label: "Changes" },
  { id: "checks", label: "Checks" },
];

/** Files, changes and checks of the workspace. */
export function RightPanel() {
  const composerMatch = useMatch("/composer/:id/*");
  const composer = useComposer(composerMatch?.params.id).data;
  const activeRun = useChat((s) => (composer ? s.activeTab[composer.workspace_id] : undefined));
  if (composerMatch) {
    return composer ? <ComposerPanel composer={composer} runId={activeRun} /> : null;
  }
  return <WorkspaceRightPanel />;
}

function WorkspaceRightPanel() {
  const workspaceId = useMatch("/workspace/:id")?.params.id;
  const workspace = useWorkspace(workspaceId).data;
  const tab = useReview((s) => s.rightTab);
  const setTab = useReview((s) => s.setRightTab);
  const scope = useReview((s) => s.scope);
  const changeCount = useChanges(workspaceId, scope).data?.length ?? 0;
  const usable = workspace
    ? workspace.state !== "archived" && workspace.state !== "errored"
    : false;
  return (
    <div className="flex h-full min-h-0 flex-col bg-background">
      {workspace && usable ? (
        <NextActionBar
          workspace={workspace}
          className="pointer-events-auto relative z-30 min-h-10 flex-wrap border-b border-border px-3 py-1"
        />
      ) : null}
      <div
        role="tablist"
        aria-label="Review"
        className="pointer-events-auto relative z-30 flex h-10 shrink-0 items-center gap-1 overflow-x-auto border-b border-border px-3 [scrollbar-width:none]"
      >
        {TABS.map(({ id, label }) => (
          <button
            key={id}
            type="button"
            role="tab"
            aria-selected={tab === id}
            onClick={() => setTab(id)}
            className={
              tab === id
                ? "shrink-0 rounded-md bg-muted px-2 py-1 text-xs whitespace-nowrap text-foreground"
                : "shrink-0 rounded-md px-2 py-1 text-xs whitespace-nowrap text-muted-foreground hover:text-foreground"
            }
          >
            {label}
            {id === "changes" && changeCount > 0 ? (
              <span className="ml-1 text-muted-foreground">{changeCount}</span>
            ) : null}
          </button>
        ))}
      </div>
      {workspace && usable ? (
        <>
          <div className="flex min-h-0 flex-1 flex-col">
            {tab === "files" ? (
              <FileTree workspaceId={workspace.id} workspacePath={workspace.path} />
            ) : null}
            {tab === "changes" ? <ChangesPanel workspaceId={workspace.id} /> : null}
            {tab === "checks" ? <ChecksPanel workspaceId={workspace.id} /> : null}
          </div>
          <GraphPanel workspaceId={workspace.id} />
        </>
      ) : (
        <p className="flex flex-1 items-center justify-center p-6 text-center text-xs text-muted-foreground">
          {workspace ? "Restore this workspace to see its files." : null}
        </p>
      )}
    </div>
  );
}
