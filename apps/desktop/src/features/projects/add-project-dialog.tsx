import type { Project } from "@circles/bindings/Project";
import { Button } from "@circles/ui/components/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@circles/ui/components/dialog";
import { Input } from "@circles/ui/components/input";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@circles/ui/components/tabs";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpenIcon, GlobeIcon, Loader2Icon, LockIcon, SearchIcon } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";
import {
  useAddGithubProject,
  useAddLocalProject,
  useGithubRepos,
  useQuickStartProject,
} from "@/features/projects/api";
import { ErrorMessage } from "@/features/projects/error-message";
import { useUi } from "@/stores/ui";

type Tab = "local" | "github" | "quick";

function LocalTab({ onAdded }: { onAdded: (p: Project) => void }) {
  const add = useAddLocalProject();
  const [path, setPath] = useState("");

  const choose = async () => {
    const picked = await open({
      directory: true,
      multiple: false,
      title: "Choose a git repository",
    });
    if (typeof picked === "string") {
      setPath(picked);
      add.mutate(picked, { onSuccess: onAdded });
    }
  };

  return (
    <div className="flex min-w-0 flex-col gap-3">
      <p className="text-sm text-muted-foreground">
        Add a repository that is already on this Mac. Circles never changes it.
      </p>
      <Button onClick={choose} disabled={add.isPending} className="w-fit">
        {add.isPending ? <Loader2Icon className="animate-spin" /> : <FolderOpenIcon />}
        Choose folder
      </Button>
      <form
        className="flex gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          if (path.trim()) add.mutate(path.trim(), { onSuccess: onAdded });
        }}
      >
        <Input
          value={path}
          onChange={(e) => setPath(e.target.value)}
          placeholder="/Users/you/code/my-repo"
          aria-label="Repository path"
          className="font-mono text-xs"
        />
        <Button type="submit" variant="outline" disabled={!path.trim() || add.isPending}>
          Add
        </Button>
      </form>
      <ErrorMessage error={add.error} />
    </div>
  );
}

function GithubTab({ active, onAdded }: { active: boolean; onAdded: (p: Project) => void }) {
  const repos = useGithubRepos(active);
  const add = useAddGithubProject();
  const [filter, setFilter] = useState("");
  const [reference, setReference] = useState("");
  const [cloning, setCloning] = useState<string | null>(null);

  const clone = (ref: string) => {
    setCloning(ref);
    add.mutate(ref, { onSuccess: onAdded, onSettled: () => setCloning(null) });
  };

  const visible = (repos.data ?? []).filter((r) =>
    r.name_with_owner.toLowerCase().includes(filter.trim().toLowerCase()),
  );

  return (
    <div className="flex min-w-0 flex-col gap-3">
      <p className="text-sm text-muted-foreground">
        Clones the repository into <span className="font-mono text-xs">~/circles/repos</span> using
        the GitHub CLI.
      </p>
      <form
        className="flex gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          if (reference.trim()) clone(reference.trim());
        }}
      >
        <Input
          value={reference}
          onChange={(e) => setReference(e.target.value)}
          placeholder="owner/name or GitHub URL"
          aria-label="Repository"
          className="font-mono text-xs"
        />
        <Button type="submit" variant="outline" disabled={!reference.trim() || add.isPending}>
          Clone
        </Button>
      </form>

      <div className="relative">
        <SearchIcon className="pointer-events-none absolute top-2.5 left-2.5 size-3.5 text-muted-foreground" />
        <Input
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          placeholder="Filter your repositories"
          aria-label="Filter your repositories"
          className="pl-8"
        />
      </div>

      <div className="max-h-56 min-w-0 overflow-x-hidden overflow-y-auto rounded-md border border-border">
        {repos.isLoading ? (
          <p className="flex items-center gap-2 p-3 text-sm text-muted-foreground">
            <Loader2Icon className="size-3.5 animate-spin" />
            Loading repositories
          </p>
        ) : repos.error ? (
          <div className="p-3">
            <ErrorMessage error={repos.error} />
          </div>
        ) : visible.length === 0 ? (
          <p className="p-3 text-sm text-muted-foreground">No repositories found.</p>
        ) : (
          <ul>
            {visible.map((repo) => (
              <li key={repo.name_with_owner} className="border-b border-border last:border-b-0">
                <button
                  type="button"
                  disabled={add.isPending}
                  onClick={() => clone(repo.name_with_owner)}
                  className="flex w-full items-center gap-2 px-3 py-2 text-left text-sm outline-none hover:bg-muted focus-visible:bg-muted disabled:opacity-60"
                >
                  <img src={repo.avatar_url} alt="" className="size-5 shrink-0 rounded-full" />
                  <span className="min-w-0 flex-1 overflow-hidden">
                    <span className="block truncate">{repo.name_with_owner}</span>
                    {repo.description ? (
                      <span className="block truncate text-xs text-muted-foreground">
                        {repo.description}
                      </span>
                    ) : null}
                  </span>
                  {cloning === repo.name_with_owner ? (
                    <Loader2Icon className="size-3.5 animate-spin" />
                  ) : repo.is_private ? (
                    <LockIcon className="size-3.5 text-muted-foreground" />
                  ) : (
                    <GlobeIcon className="size-3.5 text-muted-foreground" />
                  )}
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
      {add.isPending ? (
        <p className="text-sm text-muted-foreground">Cloning {cloning}. This can take a minute.</p>
      ) : null}
      <ErrorMessage error={add.error} />
    </div>
  );
}

function QuickStartTab({ onAdded }: { onAdded: (p: Project) => void }) {
  const create = useQuickStartProject();
  const [name, setName] = useState("");

  return (
    <form
      className="flex flex-col gap-3"
      onSubmit={(e) => {
        e.preventDefault();
        if (name.trim()) create.mutate(name.trim(), { onSuccess: onAdded });
      }}
    >
      <p className="text-sm text-muted-foreground">
        Creates a new repository in <span className="font-mono text-xs">~/circles/repos</span> with
        an initial commit.
      </p>
      <Input
        value={name}
        onChange={(e) => setName(e.target.value)}
        placeholder="my-new-project"
        aria-label="Project name"
        className="font-mono text-xs"
      />
      <Button type="submit" disabled={!name.trim() || create.isPending} className="w-fit">
        {create.isPending ? <Loader2Icon className="animate-spin" /> : null}
        Create project
      </Button>
      <ErrorMessage error={create.error} />
    </form>
  );
}

export function AddProjectDialog() {
  const isOpen = useUi((s) => s.addProjectOpen);
  const setOpen = useUi((s) => s.setAddProjectOpen);
  const initialTab = useUi((s) => s.addProjectTab);
  const [tab, setTab] = useState<Tab>("local");

  useEffect(() => {
    if (isOpen) setTab(initialTab);
  }, [isOpen, initialTab]);

  const onAdded = (project: Project) => {
    toast.success(`Added ${project.name}`, { description: project.path });
    setOpen(false);
  };

  return (
    <Dialog open={isOpen} onOpenChange={(o) => setOpen(o)}>
      <DialogContent className="min-w-0 overflow-hidden sm:max-w-lg *:min-w-0">
        <DialogHeader>
          <DialogTitle>Add project</DialogTitle>
          <DialogDescription>
            Open a repository, clone one from GitHub, or start fresh.
          </DialogDescription>
        </DialogHeader>
        <Tabs value={tab} onValueChange={(v) => setTab(v as Tab)} className="min-w-0 gap-4">
          <TabsList>
            <TabsTrigger value="local">Local folder</TabsTrigger>
            <TabsTrigger value="github">GitHub</TabsTrigger>
            <TabsTrigger value="quick">Quick start</TabsTrigger>
          </TabsList>
          <TabsContent value="local" className="min-w-0">
            <LocalTab onAdded={onAdded} />
          </TabsContent>
          <TabsContent value="github" className="min-w-0">
            <GithubTab active={isOpen && tab === "github"} onAdded={onAdded} />
          </TabsContent>
          <TabsContent value="quick" className="min-w-0">
            <QuickStartTab onAdded={onAdded} />
          </TabsContent>
        </Tabs>
      </DialogContent>
    </Dialog>
  );
}
