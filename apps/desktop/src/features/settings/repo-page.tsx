import type { Project } from "@circles/bindings/Project";
import { Button } from "@circles/ui/components/button";
import { Input } from "@circles/ui/components/input";
import { cn } from "@circles/ui/utils";
import { useState } from "react";
import { useNavigate } from "react-router";
import { useProjects, useSetBaseBranch } from "@/features/projects/api";
import { RemoveProjectDialog } from "@/features/projects/remove-project-dialog";
import { useSettingsView, type WriteScope } from "./api";
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
