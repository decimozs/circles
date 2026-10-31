import { cn } from "@circles/ui/utils";

/** A GitHub avatar and a name. Nothing is shown for an author that is not known. */
export function Person({
  name,
  avatar,
  className,
}: {
  name: string;
  avatar: string;
  className?: string;
}) {
  return (
    <span className={cn("inline-flex min-w-0 items-center gap-1.5", className)}>
      {name && avatar ? (
        <img src={avatar} alt="" className="size-4 shrink-0 rounded-full bg-muted object-cover" />
      ) : null}
      <span className="truncate">{name || "unknown"}</span>
    </span>
  );
}
