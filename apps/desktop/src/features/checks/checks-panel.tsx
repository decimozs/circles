import type { Check } from "@circles/bindings/Check";
import type { CheckBucket } from "@circles/bindings/CheckBucket";
import type { ReviewThread } from "@circles/bindings/ReviewThread";
import type { SectionError } from "@circles/bindings/SectionError";
import { Button } from "@circles/ui/components/button";
import { cn } from "@circles/ui/utils";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  CheckCircle2Icon,
  CircleDashedIcon,
  CircleSlashIcon,
  ExternalLinkIcon,
  Loader2Icon,
  RefreshCwIcon,
  XCircleIcon,
} from "lucide-react";
import { toast } from "sonner";
import {
  useChecks,
  useDeployments,
  useReadiness,
  useRefreshPr,
  useThreads,
} from "@/features/pr/api";
import { ipc } from "@/lib/ipc";
import { TodosSection } from "./todos-section";

const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

const BUCKET: Record<CheckBucket, { label: string; Icon: typeof XCircleIcon; tone: string }> = {
  pass: { label: "Passed", Icon: CheckCircle2Icon, tone: "text-success" },
  fail: { label: "Failed", Icon: XCircleIcon, tone: "text-destructive" },
  pending: { label: "Running", Icon: Loader2Icon, tone: "text-warning" },
  skipping: { label: "Skipped", Icon: CircleSlashIcon, tone: "text-muted-foreground" },
  cancel: { label: "Cancelled", Icon: CircleDashedIcon, tone: "text-muted-foreground" },
};

function Section({
  title,
  aside,
  children,
}: {
  title: string;
  aside?: string;
  children: React.ReactNode;
}) {
  return (
    <section aria-label={title} className="flex flex-col gap-1.5">
      <h3 className="flex items-center justify-between text-xs font-medium">
        <span>{title}</span>
        {aside ? <span className="font-normal text-muted-foreground">{aside}</span> : null}
      </h3>
      {children}
    </section>
  );
}

/** Why GitHub could not be asked, in place of what it would have said. */
export function GithubProblem({ error, onRetry }: { error: SectionError; onRetry: () => void }) {
  return (
    <div role="alert" className="flex flex-col gap-1.5 rounded-lg border border-border p-2 text-xs">
      <p>{error.message}</p>
      {error.fix ? (
        <code className="w-fit rounded bg-muted px-1.5 py-0.5 font-mono">{error.fix}</code>
      ) : null}
      <Button size="sm" variant="outline" className="w-fit" onClick={onRetry}>
        <RefreshCwIcon />
        Try again
      </Button>
    </div>
  );
}

function CheckRow({ check }: { check: Check }) {
  const { label, Icon, tone } = BUCKET[check.bucket];
  return (
    <li className="flex items-center gap-2 text-xs">
      <Icon
        aria-label={label}
        className={cn("size-3.5 shrink-0", tone, check.bucket === "pending" && "animate-spin")}
      />
      <span className="min-w-0 flex-1 truncate" title={check.description || check.name}>
        {check.workflow ? `${check.workflow} / ` : ""}
        {check.name}
      </span>
      {check.link ? (
        <button
          type="button"
          aria-label={`Open ${check.name}`}
          onClick={() => openUrl(check.link).catch(() => {})}
          className="shrink-0 text-muted-foreground hover:text-foreground"
        >
          <ExternalLinkIcon className="size-3.5" />
        </button>
      ) : null}
    </li>
  );
}

function ThreadRow({ workspaceId, thread }: { workspaceId: string; thread: ReviewThread }) {
  const refresh = useRefreshPr(workspaceId);
  const toggle = () =>
    ipc
      .prThreadResolve(workspaceId, thread.id, !thread.resolved)
      .then(refresh)
      .catch((e) => toast.error(message(e)));
  return (
    <li className="flex flex-col gap-1 rounded-lg border border-border p-2 text-xs">
      <div className="flex items-center gap-1.5 text-muted-foreground">
        <span className="min-w-0 flex-1 truncate font-mono">
          {thread.path}
          {thread.line ? `:${thread.line}` : ""}
        </span>
        {thread.outdated ? <span>outdated</span> : null}
        <Button size="sm" variant="ghost" className="h-6 px-2 text-xs" onClick={toggle}>
          {thread.resolved ? "Reopen" : "Resolve"}
        </Button>
      </div>
      {thread.comments.map((comment) => (
        <p
          key={comment.url || comment.body}
          className={cn("whitespace-pre-wrap", thread.resolved && "text-muted-foreground")}
        >
          <span className="font-medium">{comment.author}</span> {comment.body}
        </p>
      ))}
    </li>
  );
}

/** Where the branch stands: git, the pull request, checks, deployments, reviews and todos. */
export function ChecksPanel({ workspaceId }: { workspaceId: string }) {
  const readiness = useReadiness(workspaceId);
  const refresh = useRefreshPr(workspaceId);
  const data = readiness.data;
  const pr = data?.pr ?? null;
  const open = pr?.state === "open";
  const checks = useChecks(workspaceId, open);
  const threads = useThreads(workspaceId, open);
  const deployments = useDeployments(workspaceId, pr !== null);
  const git = data?.git;

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-3">
      <div className="flex items-center justify-between">
        <p className="text-xs text-muted-foreground">
          {readiness.isFetching ? "Updating…" : "Up to date"}
        </p>
        <Button size="sm" variant="ghost" aria-label="Refresh" onClick={refresh}>
          <RefreshCwIcon className={cn(readiness.isFetching && "animate-spin")} />
        </Button>
      </div>

      {readiness.error ? (
        <p role="alert" className="text-xs text-destructive">
          {readiness.error.message}
        </p>
      ) : null}

      {data?.gh_error ? <GithubProblem error={data.gh_error} onRetry={refresh} /> : null}

      <Section title="Git">
        {git ? (
          <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-xs">
            <dt className="text-muted-foreground">Branch</dt>
            <dd className="font-mono break-all">{git.branch}</dd>
            <dt className="text-muted-foreground">Compared with</dt>
            <dd>
              {git.ahead} ahead, {git.behind} behind {git.base}
            </dd>
            <dt className="text-muted-foreground">Working tree</dt>
            <dd>
              {git.dirty
                ? `${git.staged} staged, ${git.unstaged} changed, ${git.untracked} new`
                : "Clean"}
            </dd>
            <dt className="text-muted-foreground">Pushed</dt>
            <dd>{git.pushed ? "Yes" : "Not yet"}</dd>
            {git.conflicts > 0 || git.conflicts_with_base ? (
              <>
                <dt className="text-muted-foreground">Conflicts</dt>
                <dd className="text-destructive">
                  {git.conflicts > 0 ? `${git.conflicts} files need resolving` : `with ${git.base}`}
                </dd>
              </>
            ) : null}
          </dl>
        ) : (
          <p className="text-xs text-muted-foreground">Loading…</p>
        )}
      </Section>

      <Section title="Pull request">
        {pr ? (
          <div className="flex flex-col gap-1 text-xs">
            <button
              type="button"
              onClick={() => openUrl(pr.url).catch(() => {})}
              className="flex items-center gap-1 text-left font-medium hover:underline"
            >
              #{pr.number} {pr.title}
              <ExternalLinkIcon className="size-3 shrink-0" />
            </button>
            <p className="text-muted-foreground">
              {pr.state === "open"
                ? pr.is_draft
                  ? "Draft"
                  : "Open"
                : pr.state === "merged"
                  ? "Merged"
                  : "Closed"}
              {pr.review_decision ? `, ${pr.review_decision.replace("_", " ")}` : ""}
              {pr.mergeable === "conflicting" ? ", has conflicts" : ""}
            </p>
          </div>
        ) : (
          <p className="text-xs text-muted-foreground">
            {data?.gh_error ? "Could not look for a pull request." : "No pull request yet."}
          </p>
        )}
      </Section>

      {open ? (
        <Section
          title="Checks"
          aside={
            data && data.checks.total > 0
              ? `${data.checks.passed} of ${data.checks.total} passed`
              : undefined
          }
        >
          {checks.error ? (
            <p role="alert" className="text-xs text-destructive">
              {checks.error.message}
            </p>
          ) : checks.data && checks.data.length > 0 ? (
            <ul className="flex flex-col gap-1">
              {checks.data.map((check) => (
                <CheckRow key={`${check.workflow}/${check.name}`} check={check} />
              ))}
            </ul>
          ) : (
            <p className="text-xs text-muted-foreground">
              {checks.isLoading ? "Loading…" : "No checks reported."}
            </p>
          )}
        </Section>
      ) : null}

      {pr ? (
        <Section title="Deployments">
          {deployments.error ? (
            <p className="text-xs text-muted-foreground">Deployments are not available.</p>
          ) : deployments.data && deployments.data.length > 0 ? (
            <ul className="flex flex-col gap-1 text-xs">
              {deployments.data.map((d) => (
                <li key={`${d.environment}-${d.created_at}`} className="flex items-center gap-2">
                  <span className="min-w-0 flex-1 truncate">{d.environment}</span>
                  <span className="text-muted-foreground">{d.state.replace("_", " ")}</span>
                  {d.url ? (
                    <button
                      type="button"
                      aria-label={`Open ${d.environment}`}
                      onClick={() => openUrl(d.url as string).catch(() => {})}
                      className="text-muted-foreground hover:text-foreground"
                    >
                      <ExternalLinkIcon className="size-3.5" />
                    </button>
                  ) : null}
                </li>
              ))}
            </ul>
          ) : (
            <p className="text-xs text-muted-foreground">
              {deployments.isLoading ? "Loading…" : "No deployments."}
            </p>
          )}
        </Section>
      ) : null}

      {open ? (
        <Section
          title="Review threads"
          aside={
            threads.data
              ? `${threads.data.filter((t) => !t.resolved).length} unresolved`
              : undefined
          }
        >
          {threads.error ? (
            <p role="alert" className="text-xs text-destructive">
              {threads.error.message}
            </p>
          ) : threads.data && threads.data.length > 0 ? (
            <ul className="flex flex-col gap-1.5">
              {threads.data.map((thread) => (
                <ThreadRow key={thread.id} workspaceId={workspaceId} thread={thread} />
              ))}
            </ul>
          ) : (
            <p className="text-xs text-muted-foreground">
              {threads.isLoading ? "Loading…" : "No review comments."}
            </p>
          )}
        </Section>
      ) : null}

      <TodosSection workspaceId={workspaceId} />
    </div>
  );
}
