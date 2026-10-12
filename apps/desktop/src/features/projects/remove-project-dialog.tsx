import type { Project } from "@circles/bindings/Project";
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
import { useState } from "react";
import { toast } from "sonner";
import { useRemoveProject } from "@/features/projects/api";
import { ErrorMessage } from "@/features/projects/error-message";

type Choice = "keep" | "delete";

/**
 * Removes a project at once, with or without its workspaces. The name has to be typed first, so it
 * cannot happen by accident. The repository itself is never touched.
 */
export function RemoveProjectDialog({
  project,
  open,
  onOpenChange,
  onRemoved,
}: {
  project: Project;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onRemoved?: () => void;
}) {
  const remove = useRemoveProject();
  const [typed, setTyped] = useState("");
  const [choice, setChoice] = useState<Choice>("keep");
  const count = project.workspace_count;
  const matches = typed.trim() === project.name;

  const close = (next: boolean) => {
    if (!next) {
      setTyped("");
      setChoice("keep");
      remove.reset();
    }
    onOpenChange(next);
  };

  return (
    <Dialog open={open} onOpenChange={close}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Remove {project.name}?</DialogTitle>
          <DialogDescription>
            Removes the project from Circles. Your repository is not touched.
            <span className="mt-1 block font-mono text-xs break-all">{project.path}</span>
          </DialogDescription>
        </DialogHeader>

        {count > 0 ? (
          <fieldset className="flex flex-col gap-2 text-sm">
            <legend className="mb-1 font-medium">
              Its {count} workspace{count === 1 ? "" : "s"}
            </legend>
            {(
              [
                [
                  "keep",
                  "Keep the workspaces",
                  "Circles stops showing them. Their folders stay on your Mac, with everything in them.",
                ],
                [
                  "delete",
                  "Delete the workspaces too",
                  "Their folders are removed, with anything that was not committed. Branches are kept.",
                ],
              ] as const
            ).map(([value, title, hint]) => (
              <label
                key={value}
                className="flex cursor-pointer items-start gap-2 rounded-md border border-border p-2 has-[:checked]:border-ring"
              >
                <input
                  type="radio"
                  name="workspaces"
                  className="mt-1"
                  checked={choice === value}
                  onChange={() => setChoice(value)}
                />
                <span className="flex flex-col">
                  <span
                    className={value === "delete" ? "font-medium text-destructive" : "font-medium"}
                  >
                    {title}
                  </span>
                  <span className="text-xs text-muted-foreground">{hint}</span>
                </span>
              </label>
            ))}
          </fieldset>
        ) : null}

        <div className="flex flex-col gap-1.5 text-sm">
          <span>
            Type <strong className="font-mono">{project.name}</strong> to confirm
          </span>
          <Input
            aria-label="Project name"
            value={typed}
            autoComplete="off"
            spellCheck={false}
            onChange={(e) => setTyped(e.target.value)}
            placeholder={project.name}
          />
        </div>

        <ErrorMessage error={remove.error} />
        <DialogFooter>
          <Button variant="outline" onClick={() => close(false)}>
            Cancel
          </Button>
          <Button
            variant="destructive"
            disabled={!matches || remove.isPending}
            onClick={() =>
              remove.mutate(
                { id: project.id, workspaces: count > 0 ? choice : undefined },
                {
                  onSuccess: () => {
                    close(false);
                    toast(`Removed ${project.name}`, {
                      description:
                        count > 0 && choice === "delete"
                          ? "Its workspaces were deleted. The repository was left as it was."
                          : "The repository on disk was left as it was.",
                    });
                    onRemoved?.();
                  },
                },
              )
            }
          >
            {count > 0 && choice === "delete" ? "Remove and delete workspaces" : "Remove project"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
