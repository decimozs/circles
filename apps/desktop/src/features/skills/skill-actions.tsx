import type { Project } from "@amplify/bindings/Project";
import type { SkillRow } from "@amplify/bindings/SkillRow";
import { Button } from "@amplify/ui/components/button";
import { openUrl } from "@tauri-apps/plugin-opener";
import { CheckIcon, CopyIcon, ExternalLinkIcon, FileTextIcon, FolderOpenIcon } from "lucide-react";
import { toast } from "sonner";
import { useCopy } from "@/features/chat/markdown";
import { describeError } from "@/features/chat/use-session";
import { useProjects } from "@/features/projects/api";
import { ipc } from "@/lib/ipc";

/** The address of a project's skill on GitHub, on the project's default branch. `null` when the
 * project is not on GitHub or the skill is not inside its folder. */
export function githubUrl(project: Project | undefined, skillPath: string): string | null {
  const remote = project?.remote_url;
  if (!project || !remote) return null;
  const m = /github\.com[:/]+([^/]+)\/([^/]+?)(?:\.git)?\/?$/.exec(remote.trim());
  if (!m) return null;
  const root = project.path.endsWith("/") ? project.path : `${project.path}/`;
  if (!skillPath.startsWith(root)) return null;
  const rel = skillPath.slice(root.length).split("/").map(encodeURIComponent).join("/");
  return `https://github.com/${m[1]}/${m[2]}/tree/${encodeURIComponent(project.default_branch)}/${rel}`;
}

const fail = (e: unknown) => toast.error(describeError(e));

/** What can be done with a skill: copy the command that names it, find it, open its text. */
export function SkillActions({ row }: { row: SkillRow }) {
  const project = useProjects().data?.find((p) => p.id === row.project_id);
  const url = row.group === "project" ? githubUrl(project, row.path) : null;
  const { copied, copy } = useCopy();
  return (
    <div className="flex items-center gap-1">
      <Button
        variant="ghost"
        size="xs"
        title="Copy the command, to paste in a chat"
        onClick={() => copy(`/${row.name}`)}
      >
        {copied ? <CheckIcon /> : <CopyIcon />}
        {copied ? "Copied" : `Copy /${row.name}`}
      </Button>
      <Button
        variant="ghost"
        size="xs"
        title="Show the folder in Finder"
        onClick={() => ipc.skillsReveal(row.id).catch(fail)}
      >
        <FolderOpenIcon />
        Reveal
      </Button>
      <Button
        variant="ghost"
        size="xs"
        title="Open SKILL.md in your text editor"
        onClick={() => ipc.skillsOpen(row.id).catch(fail)}
      >
        <FileTextIcon />
        Open
      </Button>
      {url ? (
        <Button variant="ghost" size="xs" onClick={() => openUrl(url).catch(() => {})}>
          <ExternalLinkIcon />
          GitHub
        </Button>
      ) : null}
    </div>
  );
}
