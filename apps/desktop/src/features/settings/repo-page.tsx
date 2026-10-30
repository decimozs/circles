import type { Learned } from "@circles/bindings/Learned";
import type { Project } from "@circles/bindings/Project";
import type { SettingsView } from "@circles/bindings/SettingsView";
import { Button } from "@circles/ui/components/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@circles/ui/components/dialog";
import { Input } from "@circles/ui/components/input";
import { cn } from "@circles/ui/utils";
import { Loader2Icon, SparklesIcon } from "lucide-react";
import { useState } from "react";
import { useNavigate } from "react-router";
import { toast } from "sonner";
import { useProjects, useSetBaseBranch } from "@/features/projects/api";
import { RemoveProjectDialog } from "@/features/projects/remove-project-dialog";
import { ipc } from "@/lib/ipc";
import { useSetSetting, useSettingsView, valueAt, type WriteScope } from "./api";
import type { SettingDef } from "./defs";
import { DefRow, SettingRow } from "./rows";

const text = (
  id: string,
  title: string,
  description: string,
  path: string[],
  mono = true,
): SettingDef => ({
  id,
  page: "repo",
  title,
  description,
  path,
  control: { kind: "textarea", placeholder: "", mono },
});

const RUN_DEF = text("repo-run", "Run script", "Started from the Run button of a workspace.", [
  "scripts",
  "run",
]);

const REPO_DEFS: SettingDef[] = [
  text("repo-setup", "Setup script", "Runs once in a new workspace, after the files are copied.", [
    "scripts",
    "setup",
  ]),
  text("repo-archive", "Archive script", "Runs before a workspace is archived.", [
    "scripts",
    "archive",
  ]),
  {
    id: "repo-run-mode",
    page: "repo",
    title: "Run script mode",
    description: "Whether the run script can start in several workspaces at once.",
    path: ["scripts", "run_mode"],
    control: {
      kind: "select",
      options: [
        { value: "concurrent", label: "Concurrent" },
        { value: "nonconcurrent", label: "One at a time" },
      ],
      default: "concurrent",
    },
  },
  {
    id: "repo-files",
    page: "repo",
    title: "Files to copy",
    description:
      "Ignored files copied into new workspaces, one pattern per line (for example .env).",
    path: ["files_to_copy"],
    control: { kind: "list", placeholder: ".env" },
  },
  {
    id: "repo-merge",
    page: "repo",
    title: "Merge method",
    description: "How pull requests of this repository are merged.",
    path: ["git", "merge_method"],
    control: {
      kind: "select",
      options: [
        { value: "squash", label: "Squash" },
        { value: "merge", label: "Merge commit" },
        { value: "rebase", label: "Rebase" },
      ],
      default: "squash",
    },
  },
  text(
    "repo-general",
    "General instructions",
    "Added to every prompt Circles sends in this repository.",
    ["instructions", "general"],
    false,
  ),
  text(
    "repo-review",
    "Review instructions",
    "How reviews should be done.",
    ["instructions", "review"],
    false,
  ),
  text(
    "repo-pr",
    "Pull request instructions",
    "How to write the title and description of a pull request.",
    ["instructions", "pr"],
    false,
  ),
  text(
    "repo-fix",
    "Fix errors instructions",
    "How failing checks should be fixed.",
    ["instructions", "fix_errors"],
    false,
  ),
  text(
    "repo-conflicts",
    "Conflict instructions",
    "How merge conflicts should be resolved.",
    ["instructions", "conflicts"],
    false,
  ),
  text(
    "repo-rename",
    "Branch rename instructions",
    "How branches should be named.",
    ["instructions", "branch_rename"],
    false,
  ),
  text(
    "repo-commits",
    "Commit instructions",
    "How commit messages should look.",
    ["instructions", "commits"],
    false,
  ),
];

/** What Claude can propose from a repository, where each goes in the settings. */
const LEARNED: { key: keyof Learned; title: string; path: string[] }[] = [
  { key: "branch_pattern", title: "Branch naming", path: ["git", "branch_pattern"] },
  { key: "pr", title: "Pull request instructions", path: ["instructions", "pr"] },
  { key: "commits", title: "Commit instructions", path: ["instructions", "commits"] },
];

/** Has Claude read the repository's habits and propose the settings that keep them. */
function LearnRow({
  projectId,
  view,
  scope,
}: {
  projectId: string;
  view: SettingsView;
  scope: WriteScope;
}) {
  const set = useSetSetting(projectId);
  const [learning, setLearning] = useState(false);
  const [found, setFound] = useState<Learned | null>(null);
  const [skipped, setSkipped] = useState<Set<string>>(new Set());
  const [applying, setApplying] = useState(false);

  const proposals = found ? LEARNED.filter((item) => found[item.key]) : [];
  const chosen = proposals.filter((item) => !skipped.has(item.key));

  const learn = async () => {
    setLearning(true);
    try {
      const learned = await ipc.projectLearnConventions(projectId);
      if (!LEARNED.some((item) => learned[item.key])) {
        toast("The repository does not show a clear convention to keep.");
        return;
      }
      setSkipped(new Set());
      setFound(learned);
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setLearning(false);
    }
  };

  const apply = async () => {
    if (!found) return;
    setApplying(true);
    try {
      for (const item of chosen) {
        await set.mutateAsync({ scope, path: item.path, value: found[item.key] });
      }
      toast(`Saved ${chosen.length} ${chosen.length === 1 ? "setting" : "settings"}`);
      setFound(null);
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setApplying(false);
    }
  };

  return (
    <>
      <SettingRow
        title="Learn this repository"
        description="Claude reads the pull request template, AGENTS.md, CLAUDE.md, CONTRIBUTING.md, and your recent commits and branches, then proposes how to keep the branch naming and the pull request and commit instructions. You choose what to save."
      >
        <Button variant="outline" size="sm" onClick={learn} disabled={learning}>
          {learning ? <Loader2Icon className="animate-spin" /> : <SparklesIcon />}
          {learning ? "Reading" : "Learn"}
        </Button>
      </SettingRow>
      <Dialog open={found !== null} onOpenChange={(open) => !open && setFound(null)}>
        <DialogContent className="flex max-h-[85vh] flex-col gap-0 p-0 sm:max-w-xl">
          <DialogHeader className="shrink-0 border-b border-border px-5 py-3">
            <DialogTitle>What Claude learned</DialogTitle>
            <DialogDescription>
              Saved to {scope === "project" ? "the repository's shared settings" : "this Mac only"}.
              A saved value replaces the one you have now.
            </DialogDescription>
          </DialogHeader>
          <ul className="min-h-0 flex-1 divide-y divide-border overflow-y-auto">
            {proposals.map((item) => {
              const current = valueAt(view.settings, item.path);
              return (
                <li key={item.key} className="flex gap-3 px-5 py-3 text-sm">
                  <input
                    type="checkbox"
                    aria-label={`Save ${item.title}`}
                    checked={!skipped.has(item.key)}
                    onChange={(e) =>
                      setSkipped((prev) => {
                        const next = new Set(prev);
                        if (e.target.checked) next.delete(item.key);
                        else next.add(item.key);
                        return next;
                      })
                    }
                    className="mt-1 size-4 shrink-0 accent-current"
                  />
                  <div className="flex min-w-0 flex-1 flex-col gap-1">
                    <span className="font-medium">{item.title}</span>
                    <p className="whitespace-pre-wrap rounded-md bg-muted px-2 py-1.5 font-mono text-xs">
                      {found?.[item.key]}
                    </p>
                    {typeof current === "string" && current.trim() ? (
                      <p className="text-xs text-muted-foreground">
                        Now: <span className="font-mono">{current}</span>
                      </p>
                    ) : null}
                  </div>
                </li>
              );
            })}
          </ul>
          <DialogFooter className="shrink-0 border-t border-border px-5 py-3">
            <Button variant="outline" onClick={() => setFound(null)}>
              Cancel
            </Button>
            <Button onClick={apply} disabled={applying || chosen.length === 0}>
              {applying ? <Loader2Icon className="animate-spin" /> : null}
              Save {chosen.length}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

function BaseBranch({ project }: { project: Project }) {
  const set = useSetBaseBranch();
  const [value, setValue] = useState(project.default_branch);
  return (
    <SettingRow
      title="Base branch"
      description="New workspaces branch from here, and pull requests target it."
      htmlFor="repo-base"
      error={set.error ? (set.error as Error).message : null}
    >
      <Input
        id="repo-base"
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onBlur={() =>
          value.trim() &&
          value !== project.default_branch &&
          set.mutate({ id: project.id, branch: value.trim() })
        }
        className="h-8 w-48 font-mono text-xs"
      />
    </SettingRow>
  );
}

function RemoveRow({ project }: { project: Project }) {
  const navigate = useNavigate();
  const [open, setOpen] = useState(false);
  return (
    <>
      <SettingRow
        title="Remove project"
        description="Take it out of Circles, with or without its workspaces. The repository folder is never touched."
      >
        <Button variant="destructive" size="sm" onClick={() => setOpen(true)}>
          Remove…
        </Button>
      </SettingRow>
      <RemoveProjectDialog
        project={project}
        open={open}
        onOpenChange={setOpen}
        onRemoved={() => navigate("/settings/general")}
      />
    </>
  );
}

/** Settings of one repository. They are kept for this Mac, or shared in `.circles/settings.toml`. */
export function RepoSettings({ projectId }: { projectId: string }) {
  const project = useProjects().data?.find((p) => p.id === projectId);
  const view = useSettingsView(projectId).data;
  const [shared, setShared] = useState(false);
  if (!project || !view) return null;
  const scope: WriteScope = shared ? "project" : "local";
  return (
    <div>
      <div className="mb-2 flex items-center justify-between gap-4 border-b border-border pb-3">
        <div>
          <p className="text-sm font-medium">Save changes to</p>
          <p className="text-xs text-muted-foreground">
            {shared
              ? "The repository's .circles/settings.toml, shared with everyone who clones it."
              : "This Mac only."}
          </p>
        </div>
        <fieldset className="m-0 flex min-w-0 gap-0.5 rounded-lg border-0 bg-muted p-0.5">
          <legend className="sr-only">Save changes to</legend>
          {(
            [
              [false, "Only me"],
              [true, "Shared"],
            ] as const
          ).map(([value, label]) => (
            <button
              key={label}
              type="button"
              aria-pressed={shared === value}
              onClick={() => setShared(value)}
              className={cn(
                "rounded-md px-2.5 py-1 text-xs",
                shared === value ? "bg-background shadow-sm" : "text-muted-foreground",
              )}
            >
              {label}
            </button>
          ))}
        </fieldset>
      </div>
      <LearnRow projectId={projectId} view={view} scope={scope} />
      <BaseBranch project={project} />
      {REPO_DEFS.map((def) => (
        <div key={def.id}>
          <DefRow def={def} view={view} projectId={projectId} scope={scope} />
          {def.id === "repo-setup" ? (
            typeof view.settings.scripts.run === "object" && view.settings.scripts.run !== null ? (
              <p className="py-2 text-xs text-muted-foreground">
                The run scripts are named in the settings file. Open the file to edit them.
              </p>
            ) : (
              <DefRow def={RUN_DEF} view={view} projectId={projectId} scope={scope} />
            )
          ) : null}
        </div>
      ))}
      <RemoveRow project={project} />
    </div>
  );
}
