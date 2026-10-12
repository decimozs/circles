import type { Project } from "@circles/bindings/Project";
import { cn } from "@circles/ui/utils";
import { useState } from "react";

/** GitHub owner avatar, or the first letter of the name. */
export function ProjectAvatar({
  project,
  className,
}: {
  project: Pick<Project, "name" | "avatar_url">;
  className?: string;
}) {
  const [failed, setFailed] = useState(false);

  if (project.avatar_url && !failed) {
    return (
      <img
        src={project.avatar_url}
        alt=""
        onError={() => setFailed(true)}
        className={cn("size-5 shrink-0 rounded-full bg-muted object-cover", className)}
      />
    );
  }
  return (
    <span
      aria-hidden
      className={cn(
        "flex size-5 shrink-0 items-center justify-center rounded-full bg-muted text-2xs font-medium uppercase",
        className,
      )}
    >
      {project.name.slice(0, 1)}
    </span>
  );
}
