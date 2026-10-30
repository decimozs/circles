import type { AppInfo } from "@circles/bindings/AppInfo";
import { Button } from "@circles/ui/components/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@circles/ui/components/dialog";
import { useEffect, useRef, useState } from "react";
import { Link, Navigate, useParams } from "react-router";
import { ChatPanel } from "@/features/chat/chat-panel";
import { DiffView } from "@/features/diff/diff-view";
import { useFilesWatcher } from "@/features/files/api";
import { FileView } from "@/features/files/file-view";
import { useReadinessOnFileChanges } from "@/features/pr/api";
import { ErrorMessage } from "@/features/projects/error-message";
import { ProjectList } from "@/features/projects/project-list";
import { useRestoreWorkspace, useWorkspace } from "@/features/workspaces/api";
import { WorkspaceList } from "@/features/workspaces/workspace-list";
import { IpcError, ipc } from "@/lib/ipc";
import { useReview } from "@/stores/review";

export function WorkspacePage() {
  const { id } = useParams();
  const workspace = useWorkspace(id);
  const restore = useRestoreWorkspace();
  const view = useReview((s) => (id ? s.views[id] : undefined));
  // Focus starts on the viewer itself, not on a button, so Escape closes the dialog at once.
  const viewerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (id) ipc.workspaceOpen(id).catch(() => {});
  }, [id]);
  useFilesWatcher(id);
  useReadinessOnFileChanges(id);

  // A workspace that was deleted leaves its page for Home, which offers to add a project.
  if (workspace.error instanceof IpcError && workspace.error.code === "not_found") {
    return <Navigate to="/" replace />;
  }
  if (workspace.error) {
    return (
      <div className="mx-auto max-w-2xl px-6 py-8">
        <ErrorMessage error={workspace.error} />
      </div>
    );
  }
  const ws = workspace.data;
  if (!ws) return null;

  const usable = ws.state !== "errored" && ws.state !== "archived";
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="mx-auto flex w-full max-w-3xl shrink-0 flex-col gap-3 px-6 pt-4">
        {!usable ? (
          <div
            role="status"
            className="flex items-center justify-between gap-3 rounded-md border border-border bg-muted/50 p-3 text-sm"
          >
            <span>
              {ws.state === "archived"
                ? `Archived. The branch ${ws.branch} was kept.`
                : (ws.error ?? "Something went wrong.")}
            </span>
            <Button size="sm" disabled={restore.isPending} onClick={() => restore.mutate(ws.id)}>
              Restore
            </Button>
          </div>
        ) : null}
        <ErrorMessage error={restore.error} />
      </div>
      <div className="min-h-0 flex-1">
        <ChatPanel workspace={ws} />
      </div>
      {/* A file or a diff opens over the chat, so the conversation stays where it was. */}
      <Dialog
        open={view !== undefined}
        disablePointerDismissal
        onOpenChange={(open) => !open && useReview.getState().closeView(ws.id)}
      >
        <DialogContent
          showCloseButton={false}
          initialFocus={viewerRef}
          className="flex h-[88vh] flex-col gap-0 overflow-hidden p-0 sm:max-w-[min(1280px,94vw)]"
        >
          <DialogTitle className="sr-only">
            {view?.kind === "diff" ? `Changes in ${view.path}` : `File ${view?.path ?? ""}`}
          </DialogTitle>
          <DialogDescription className="sr-only">
            Read or review the file, and leave a comment on a line.
          </DialogDescription>
          <div ref={viewerRef} tabIndex={-1} className="min-h-0 flex-1 outline-none">
            {view?.kind === "diff" ? (
              <DiffView workspaceId={ws.id} path={view.path} oldPath={view.oldPath} />
            ) : view?.kind === "file" ? (
              <FileView workspaceId={ws.id} path={view.path} />
            ) : null}
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}

export function NotFoundPage() {
  return (
    <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
      Page not found.
    </div>
  );
}

export function DebugPage() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    ipc
      .getAppInfo()
      .then(setInfo)
      .catch((e: Error) => setError(e.message));
  }, []);

  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-4 px-6 py-8">
      <h1 className="text-xl font-semibold">Diagnostics</h1>
      <Link to="/debug/agent" className="text-sm underline">
        Agent debug: talk to the agent in a workspace
      </Link>
      <ProjectList />
      <WorkspaceList />
      {error ? <p className="text-sm text-destructive">{error}</p> : null}
      {info ? (
        <>
          <section className="flex flex-col gap-1 text-sm">
            <h2 className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              App
            </h2>
            <p>
              Circles {info.version} on {info.platform} ({info.arch})
            </p>
            <p className="text-muted-foreground">
              Database schema v{info.db.schema_version}, journal mode {info.db.journal_mode}
            </p>
          </section>

          <section className="flex flex-col gap-1 text-sm">
            <h2 className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              Tools
            </h2>
            {info.env.tools.map((tool) => (
              <p key={tool.name} className="font-mono text-xs">
                <span className={tool.path ? "text-success" : "text-destructive"}>
                  {tool.path ? "found" : "missing"}
                </span>{" "}
                {tool.name} {tool.path ?? ""}
              </p>
            ))}
          </section>

          <section className="flex flex-col gap-1 text-sm">
            <h2 className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              Environment
            </h2>
            <p>
              Source:{" "}
              {info.env.source.kind === "login_shell"
                ? "login shell"
                : `fallback (${info.env.source.reason})`}
              , {info.env.var_count} variables
            </p>
            <ul className="font-mono text-xs text-muted-foreground">
              {info.env.path_entries.map((entry) => (
                <li key={entry}>{entry}</li>
              ))}
            </ul>
          </section>

          <section className="flex flex-col gap-1 text-sm">
            <h2 className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              Paths
            </h2>
            <ul className="font-mono text-xs text-muted-foreground">
              {Object.entries(info.paths).map(([name, value]) => (
                <li key={name}>
                  {name}: {value}
                </li>
              ))}
            </ul>
          </section>
        </>
      ) : null}
    </div>
  );
}
