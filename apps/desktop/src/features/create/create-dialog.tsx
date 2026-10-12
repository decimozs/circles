import type { CreateSource } from "@circles/bindings/CreateSource";
import type { ProgressEvent } from "@circles/bindings/ProgressEvent";
import { Button } from "@circles/ui/components/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@circles/ui/components/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@circles/ui/components/dropdown-menu";
import { Input } from "@circles/ui/components/input";
import { Textarea } from "@circles/ui/components/textarea";
import { listen } from "@tauri-apps/api/event";
import { open as pickFiles } from "@tauri-apps/plugin-dialog";
import {
  ChevronDownIcon,
  CornerDownLeftIcon,
  GitBranchIcon,
  GitPullRequestIcon,
  Loader2Icon,
  MoreHorizontalIcon,
  PaperclipIcon,
  PlusIcon,
  XIcon,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router";
import { useProjects } from "@/features/projects/api";
import { ErrorMessage } from "@/features/projects/error-message";
import { ProjectAvatar } from "@/features/projects/project-avatar";
import { useBranches, useCreateWorkspace, usePullRequests } from "@/features/workspaces/api";
import {
  EFFORTS,
  type EffortId,
  effortLabel,
  MODELS,
  type ModelId,
  modelLabel,
} from "@/lib/models";
import { useCreatePrefs } from "@/stores/create-prefs";
import { useUi } from "@/stores/ui";

type Mode = "new" | "existing" | "pr";

const STAGE_LABEL: Record<string, string> = {
  fetching: "Fetching from origin",
  creating_worktree: "Creating the worktree",
  copying_files: "Copying files",
  finishing: "Finishing up",
};

/** The latest creation stage the backend reported for `projectId`, while `active`. */
function useCreateStage(projectId: string | null, active: boolean): string | null {
  const [stage, setStage] = useState<string | null>(null);

  useEffect(() => {
    if (!active) {
      setStage(null);
      return;
    }
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    listen<ProgressEvent>("workspace:progress", (event) => {
      if (event.payload.project_id === projectId) setStage(event.payload.stage);
    })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {
        // Not running inside Tauri: there is no progress to show.
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [active, projectId]);

  return stage;
}

function fileName(path: string): string {
  return path.split("/").pop() || path;
}

export function CreateDialog() {
  const isOpen = useUi((s) => s.createOpen);
  const setOpen = useUi((s) => s.setCreateOpen);
  const presetProject = useUi((s) => s.createProjectId);
  const setAddProjectOpen = useUi((s) => s.setAddProjectOpen);
  const navigate = useNavigate();

  const projects = useProjects().data ?? [];
  const prefs = useCreatePrefs();
  const create = useCreateWorkspace();

  const [projectId, setProjectId] = useState<string | null>(null);
  const [mode, setMode] = useState<Mode>("new");
  const [panelOpen, setPanelOpen] = useState(false);
  const [base, setBase] = useState("");
  const [branch, setBranch] = useState("");
  const [prNumber, setPrNumber] = useState("");
  const [filter, setFilter] = useState("");
  const [prompt, setPrompt] = useState("");
  const [attachments, setAttachments] = useState<string[]>([]);
  const promptRef = useRef<HTMLTextAreaElement>(null);

  // Start fresh every time the dialog opens.
  // biome-ignore lint/correctness/useExhaustiveDependencies: only reset when it opens
  useEffect(() => {
    if (!isOpen) return;
    setMode("new");
    setPanelOpen(false);
    setBase("");
    setBranch("");
    setPrNumber("");
    setFilter("");
    setPrompt("");
    setAttachments([]);
    create.reset();
  }, [isOpen]);

  // Pick the project: the one the dialog was opened for, the last one used, or the first.
  // biome-ignore lint/correctness/useExhaustiveDependencies: derived from ids only
  useEffect(() => {
    if (!isOpen) return;
    const ids = projects.map((p) => p.id);
    const wanted = [presetProject, prefs.projectId].find((id) => id && ids.includes(id));
    setProjectId(wanted ?? ids[0] ?? null);
  }, [isOpen, presetProject, projects.map((p) => p.id).join(",")]);

  const project = projects.find((p) => p.id === projectId) ?? null;
  const stage = useCreateStage(projectId, create.isPending);

  const needsBranches = isOpen && panelOpen && mode !== "pr";
  const branches = useBranches(projectId ?? undefined, needsBranches);
  const prs = usePullRequests(projectId ?? undefined, isOpen && panelOpen && mode === "pr");

  const prValue = Number.parseInt(prNumber, 10);
  const prValid = Number.isInteger(prValue) && prValue > 0;
  const sourceReady =
    mode === "new" || (mode === "existing" && branch.trim() !== "") || (mode === "pr" && prValid);
  const canCreate = !!project && sourceReady && !create.isPending;

  const source: CreateSource = useMemo(() => {
    if (mode === "existing") return { kind: "existing_branch", name: branch.trim() };
    if (mode === "pr") return { kind: "pull_request", number: prValue };
    return { kind: "new_branch", base_branch: base.trim() || null };
  }, [mode, branch, prValue, base]);

  const sourceSummary =
    mode === "existing"
      ? `Branch ${branch.trim() || "…"}`
      : mode === "pr"
        ? `Pull request ${prValid ? `#${prValue}` : "…"}`
        : `New branch from ${base.trim() || project?.default_branch || "…"}`;

  const submit = (withPrompt: boolean) => {
    if (!canCreate || !projectId) return;
    create.mutate(
      {
        projectId,
        options: {
          source,
          prompt: withPrompt && prompt.trim() ? prompt.trim() : null,
          model: prefs.model,
          effort: prefs.effort,
          attachments,
        },
      },
      {
        onSuccess: (workspace) => {
          prefs.set({ projectId });
          setOpen(false);
          navigate(`/workspace/${workspace.id}`);
        },
      },
    );
  };

  const chooseMode = (next: Mode) => {
    setMode(next);
    setPanelOpen(true);
    setFilter("");
  };

  const addAttachments = async () => {
    const picked = await pickFiles({ multiple: true, directory: false, title: "Attach files" });
    const paths = picked === null ? [] : Array.isArray(picked) ? picked : [picked];
    setAttachments((current) => [...new Set([...current, ...paths])]);
  };

  const visibleBranches = (branches.data ?? []).filter((b) =>
    b.name.toLowerCase().includes(filter.trim().toLowerCase()),
  );
  const visiblePrs = (prs.data ?? []).filter((p) =>
    `${p.number} ${p.title} ${p.author}`.toLowerCase().includes(filter.trim().toLowerCase()),
  );

  return (
    <Dialog open={isOpen} onOpenChange={setOpen}>
      <DialogContent
        showCloseButton={false}
        className="min-w-0 gap-0 overflow-hidden p-0 sm:max-w-3xl *:min-w-0"
        initialFocus={promptRef}
      >
        <DialogTitle className="sr-only">Create workspace</DialogTitle>
        <DialogDescription className="sr-only">
          Describe what you want to work on. Circles creates an isolated workspace for it.
        </DialogDescription>

        {create.isPending ? (
          <div
            role="status"
            className="flex min-h-72 flex-col items-center justify-center gap-3 px-6 text-center"
          >
            <Loader2Icon className="size-7 animate-spin text-muted-foreground" />
            <div className="flex flex-col gap-1">
              <p className="text-sm font-medium">Creating your workspace…</p>
              <p className="text-xs text-muted-foreground">
                {(stage && STAGE_LABEL[stage]) ?? "Preparing the workspace"}
              </p>
            </div>
          </div>
        ) : (
          <>
            <div className="flex h-11 items-center gap-1.5 border-b border-border px-3">
              <DropdownMenu>
                <DropdownMenuTrigger
                  render={
                    <Button
                      variant="ghost"
                      size="sm"
                      className="max-w-56 gap-1.5 px-1.5 text-sm font-medium"
                    />
                  }
                >
                  {project ? <ProjectAvatar project={project} className="size-4" /> : null}
                  <span className="truncate">{project?.name ?? "Choose a project"}</span>
                  <ChevronDownIcon className="size-3.5 text-muted-foreground" />
                </DropdownMenuTrigger>
                <DropdownMenuContent align="start" className="w-64">
                  {projects.length === 0 ? (
                    <DropdownMenuItem
                      onClick={() => {
                        setOpen(false);
                        setAddProjectOpen(true);
                      }}
                    >
                      <PlusIcon /> Add a project first
                    </DropdownMenuItem>
                  ) : (
                    <DropdownMenuRadioGroup value={projectId ?? ""} onValueChange={setProjectId}>
                      {projects.map((p) => (
                        <DropdownMenuRadioItem key={p.id} value={p.id}>
                          <ProjectAvatar project={p} className="size-4" />
                          <span className="truncate">{p.name}</span>
                        </DropdownMenuRadioItem>
                      ))}
                    </DropdownMenuRadioGroup>
                  )}
                </DropdownMenuContent>
              </DropdownMenu>

              <DropdownMenu>
                <DropdownMenuTrigger
                  render={
                    <Button
                      variant="ghost"
                      size="icon-sm"
                      aria-label="More options"
                      className="text-muted-foreground"
                    />
                  }
                >
                  <MoreHorizontalIcon />
                </DropdownMenuTrigger>
                <DropdownMenuContent align="start" className="w-60">
                  <DropdownMenuItem onClick={() => chooseMode("new")}>
                    <GitBranchIcon /> Change the base branch
                  </DropdownMenuItem>
                  <DropdownMenuItem onClick={() => chooseMode("existing")}>
                    <GitBranchIcon /> Use an existing branch
                  </DropdownMenuItem>
                  <DropdownMenuItem onClick={() => chooseMode("pr")}>
                    <GitPullRequestIcon /> Use a pull request
                  </DropdownMenuItem>
                  <DropdownMenuSeparator />
                  <DropdownMenuItem
                    onClick={() => {
                      setMode("new");
                      setBase("");
                      setBranch("");
                      setPrNumber("");
                      setPanelOpen(false);
                    }}
                  >
                    Start from the default branch
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>

              <button
                type="button"
                onClick={() => setPanelOpen((o) => !o)}
                className="ml-auto flex min-w-0 max-w-72 items-center gap-1.5 rounded-md px-2 py-1 text-xs text-muted-foreground outline-none hover:bg-muted focus-visible:bg-muted"
              >
                {mode === "pr" ? (
                  <GitPullRequestIcon className="size-3.5 shrink-0" />
                ) : (
                  <GitBranchIcon className="size-3.5 shrink-0" />
                )}
                <span className="truncate">{sourceSummary}</span>
              </button>
            </div>

            {panelOpen ? (
              <div className="flex flex-col gap-2 border-b border-border bg-muted/30 p-3">
                {mode === "new" ? (
                  <>
                    <label htmlFor="create-base" className="text-xs text-muted-foreground">
                      Base branch (leave empty to use {project?.default_branch ?? "the default"})
                    </label>
                    <Input
                      id="create-base"
                      value={base}
                      onChange={(e) => setBase(e.target.value)}
                      list="create-branch-options"
                      placeholder={project?.default_branch ?? "main"}
                      className="h-8 font-mono text-xs"
                    />
                    <datalist id="create-branch-options">
                      {(branches.data ?? []).map((b) => (
                        <option key={b.name} value={b.name} />
                      ))}
                    </datalist>
                  </>
                ) : null}

                {mode === "existing" ? (
                  <>
                    <Input
                      value={filter}
                      onChange={(e) => setFilter(e.target.value)}
                      placeholder="Filter branches"
                      aria-label="Filter branches"
                      className="h-8 text-xs"
                    />
                    <div className="max-h-40 overflow-y-auto rounded-md border border-border bg-background">
                      {branches.isLoading ? (
                        <p className="p-2 text-xs text-muted-foreground">Loading branches</p>
                      ) : branches.error ? (
                        <div className="p-2">
                          <ErrorMessage error={branches.error} />
                        </div>
                      ) : visibleBranches.length === 0 ? (
                        <p className="p-2 text-xs text-muted-foreground">No branches found.</p>
                      ) : (
                        <ul>
                          {visibleBranches.map((b) => (
                            <li key={b.name}>
                              <button
                                type="button"
                                disabled={!!b.checked_out_at}
                                aria-pressed={branch === b.name}
                                onClick={() => setBranch(b.name)}
                                className="flex w-full items-center gap-2 px-2 py-1.5 text-left text-xs outline-none hover:bg-muted focus-visible:bg-muted disabled:opacity-50 aria-pressed:bg-muted"
                              >
                                <span className="min-w-0 flex-1 truncate font-mono">{b.name}</span>
                                <span className="shrink-0 text-2xs text-muted-foreground">
                                  {b.checked_out_at ? "in use" : b.local ? "local" : "origin"}
                                </span>
                              </button>
                            </li>
                          ))}
                        </ul>
                      )}
                    </div>
                  </>
                ) : null}

                {mode === "pr" ? (
                  <>
                    <div className="flex gap-2">
                      <Input
                        value={filter}
                        onChange={(e) => setFilter(e.target.value)}
                        placeholder="Filter pull requests"
                        aria-label="Filter pull requests"
                        className="h-8 text-xs"
                      />
                      <Input
                        value={prNumber}
                        onChange={(e) => setPrNumber(e.target.value.replace(/\D/g, ""))}
                        placeholder="#"
                        aria-label="Pull request number"
                        inputMode="numeric"
                        className="h-8 w-24 font-mono text-xs"
                      />
                    </div>
                    <div className="max-h-40 overflow-y-auto rounded-md border border-border bg-background">
                      {prs.isLoading ? (
                        <p className="p-2 text-xs text-muted-foreground">Loading pull requests</p>
                      ) : prs.error ? (
                        <div className="p-2">
                          <ErrorMessage error={prs.error} />
                        </div>
                      ) : visiblePrs.length === 0 ? (
                        <p className="p-2 text-xs text-muted-foreground">
                          No open pull requests. Enter a number above.
                        </p>
                      ) : (
                        <ul>
                          {visiblePrs.map((p) => (
                            <li key={p.number}>
                              <button
                                type="button"
                                aria-pressed={prValue === p.number}
                                onClick={() => setPrNumber(String(p.number))}
                                className="flex w-full items-center gap-2 px-2 py-1.5 text-left text-xs outline-none hover:bg-muted focus-visible:bg-muted aria-pressed:bg-muted"
                              >
                                <span className="shrink-0 font-mono text-muted-foreground">
                                  #{p.number}
                                </span>
                                <span className="min-w-0 flex-1 truncate">{p.title}</span>
                                <span className="shrink-0 text-2xs text-muted-foreground">
                                  {p.is_cross_repository ? "fork · " : ""}
                                  {p.author}
                                </span>
                              </button>
                            </li>
                          ))}
                        </ul>
                      )}
                    </div>
                  </>
                ) : null}
              </div>
            ) : null}

            <Textarea
              ref={promptRef}
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              onKeyDown={(e) => {
                if (e.key !== "Enter" || e.shiftKey || e.nativeEvent.isComposing) return;
                e.preventDefault();
                if (prompt.trim()) submit(true);
              }}
              placeholder="What do you want to work on?"
              aria-label="What do you want to work on?"
              disabled={create.isPending}
              className="min-h-36 resize-none rounded-none border-0 bg-transparent px-4 py-3 text-sm shadow-none focus-visible:ring-0 dark:bg-transparent"
            />

            {attachments.length > 0 ? (
              <ul className="flex flex-wrap gap-1.5 px-4 pb-2">
                {attachments.map((path) => (
                  <li
                    key={path}
                    className="flex max-w-56 items-center gap-1 rounded-md bg-muted py-0.5 pr-0.5 pl-2 text-xs"
                  >
                    <PaperclipIcon className="size-3 shrink-0 text-muted-foreground" />
                    <span className="truncate" title={path}>
                      {fileName(path)}
                    </span>
                    <button
                      type="button"
                      aria-label={`Remove ${fileName(path)}`}
                      onClick={() => setAttachments((a) => a.filter((p) => p !== path))}
                      className="flex size-5 items-center justify-center rounded text-muted-foreground outline-none hover:text-foreground focus-visible:text-foreground"
                    >
                      <XIcon className="size-3" />
                    </button>
                  </li>
                ))}
              </ul>
            ) : null}

            {create.error ? (
              <div className="px-4 pb-2">
                <ErrorMessage error={create.error} />
              </div>
            ) : null}

            <div className="flex h-12 items-center gap-1 px-3 pb-1">
              <DropdownMenu>
                <DropdownMenuTrigger
                  render={
                    <Button
                      variant="ghost"
                      size="sm"
                      className="gap-1.5 px-2 text-xs text-muted-foreground"
                    />
                  }
                >
                  {modelLabel(prefs.model)}
                  <span className="text-muted-foreground/70">{effortLabel(prefs.effort)}</span>
                  <ChevronDownIcon className="size-3" />
                </DropdownMenuTrigger>
                <DropdownMenuContent align="start" className="w-48">
                  <DropdownMenuGroup>
                    <DropdownMenuLabel>Model</DropdownMenuLabel>
                    <DropdownMenuRadioGroup
                      value={prefs.model}
                      onValueChange={(v) => prefs.set({ model: v as ModelId })}
                    >
                      {MODELS.map((m) => (
                        <DropdownMenuRadioItem key={m.id} value={m.id}>
                          {m.label}
                        </DropdownMenuRadioItem>
                      ))}
                    </DropdownMenuRadioGroup>
                  </DropdownMenuGroup>
                  <DropdownMenuSeparator />
                  <DropdownMenuGroup>
                    <DropdownMenuLabel>Effort</DropdownMenuLabel>
                    <DropdownMenuRadioGroup
                      value={prefs.effort}
                      onValueChange={(v) => prefs.set({ effort: v as EffortId })}
                    >
                      {EFFORTS.map((e) => (
                        <DropdownMenuRadioItem key={e.id} value={e.id}>
                          {e.label}
                        </DropdownMenuRadioItem>
                      ))}
                    </DropdownMenuRadioGroup>
                  </DropdownMenuGroup>
                </DropdownMenuContent>
              </DropdownMenu>

              <div className="ml-auto flex items-center gap-1.5">
                {!prompt.trim() ? (
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={!canCreate}
                    onClick={() => submit(false)}
                    className="text-xs text-muted-foreground"
                  >
                    Create without a prompt
                  </Button>
                ) : null}
                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label="Attach files"
                  disabled={create.isPending}
                  onClick={addAttachments}
                  className="text-muted-foreground"
                >
                  <PlusIcon />
                </Button>
                <Button
                  size="sm"
                  disabled={!canCreate || !prompt.trim()}
                  onClick={() => submit(true)}
                >
                  {create.error ? "Try again" : "Create"}
                  <CornerDownLeftIcon className="size-3" />
                </Button>
              </div>
            </div>
          </>
        )}
      </DialogContent>
    </Dialog>
  );
}
