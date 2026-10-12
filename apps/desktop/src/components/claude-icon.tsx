import { cn } from "@circles/ui/utils";
import { SiAnthropic } from "react-icons/si";

/** The Anthropic mark, next to the model name. */
export function ClaudeIcon({ className }: { className?: string }) {
  return (
    <SiAnthropic aria-hidden="true" className={cn("size-3 shrink-0 text-foreground", className)} />
  );
}
