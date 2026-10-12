import type { Project } from "@circles/bindings/Project";
import { Button } from "@circles/ui/components/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@circles/ui/components/dropdown-menu";
import { Tooltip, TooltipContent, TooltipTrigger } from "@circles/ui/components/tooltip";
import { MoreHorizontalIcon, TriangleAlertIcon } from "lucide-react";
import { useState } from "react";
import { RemoveProjectDialog } from "@/features/projects/remove-project-dialog";
import { useUi } from "@/stores/ui";

/** Shown next to a project whose repository folder was moved or deleted outside Circles. */
export function MissingFolderMarker({ project }: { project: Project }) {
  if (project.path_exists) return null;
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <span role="img" aria-label="Folder not found" className="flex shrink-0 text-warning" />
        }
      >
        <TriangleAlertIcon className="size-3.5" />
      </TooltipTrigger>
      <TooltipContent>Folder not found: {project.path}</TooltipContent>
    </Tooltip>
  );
}

/** The `···` menu of a project in the sidebar, with the confirmation to remove it. */
export function ProjectMenu({ project }: { project: Project }) {
  const openCreate = useUi((s) => s.openCreate);
  const [confirming, setConfirming] = useState(false);

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger
          render={
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={`Actions for ${project.name}`}
              className="size-6 text-muted-foreground opacity-0 group-hover/project:opacity-100 focus-visible:opacity-100 data-popup-open:opacity-100"
            />
          }
        >
          <MoreHorizontalIcon />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="w-52">
          <DropdownMenuItem onClick={() => openCreate(project.id)}>New workspace</DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem className="text-destructive" onClick={() => setConfirming(true)}>
            Remove project
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

      <RemoveProjectDialog project={project} open={confirming} onOpenChange={setConfirming} />
    </>
  );
}
