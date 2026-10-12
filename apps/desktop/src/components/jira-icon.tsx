import { cn } from "@circles/ui/utils";
import { SiJira } from "react-icons/si";

/** The Jira mark, in Atlassian blue. */
export function JiraIcon({ className }: { className?: string }) {
  return (
    <SiJira
      aria-hidden="true"
      className={cn("size-4 shrink-0 text-[var(--brand-jira)]", className)}
    />
  );
}
