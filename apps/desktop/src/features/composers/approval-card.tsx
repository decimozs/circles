import type { Approval } from "@circles/bindings/Approval";
import { Button } from "@circles/ui/components/button";
import { cn } from "@circles/ui/utils";
import { ShieldQuestionIcon } from "lucide-react";
import { toast } from "sonner";
import { useAnswerApproval } from "./api";

const TIER_TEXT: Record<string, string> = {
  write: "changes something on this Mac",
  external: "is visible outside this Mac",
  destructive: "deletes or removes something",
};

/** What a call was told to do, from the stored inputs. */
function inputLines(input: string): [string, string][] {
  try {
    const parsed = JSON.parse(input) as Record<string, unknown>;
    return Object.entries(parsed).map(([k, v]) => [
      k,
      typeof v === "string" ? v : JSON.stringify(v),
    ]);
  } catch {
    return input ? [["input", input]] : [];
  }
}

export function toolTitle(tool: string): string {
  return tool.replace(/_/g, " ");
}

/** A tool call that waits for a person: what it is, and Allow, Allow for this run or Deny. */
export function ApprovalCard({ approval, className }: { approval: Approval; className?: string }) {
  const answer = useAnswerApproval();
  const lines = inputLines(approval.input);
  const send = (allow: boolean, forRun = false) =>
    answer.mutate(
      { id: approval.id, allow, forRun },
      { onError: (e) => toast.error(e instanceof Error ? e.message : String(e)) },
    );
  return (
    <fieldset
      aria-label={`Approval for ${toolTitle(approval.tool)}`}
      className={cn(
        "flex flex-col gap-2 rounded-lg border border-warning/60 bg-warning/5 p-3 text-sm",
        className,
      )}
    >
      <div className="flex items-center gap-2 font-medium">
        <ShieldQuestionIcon className="size-4 text-warning" />
        <span>Wants to {toolTitle(approval.tool)}</span>
      </div>
      <p className="text-xs text-muted-foreground">
        This {TIER_TEXT[approval.tier] ?? "needs your approval"}.
      </p>
      {lines.length > 0 ? (
        <dl className="max-h-40 overflow-auto rounded-md bg-muted/60 p-2 font-mono text-xs">
          {lines.map(([key, value]) => (
            <div key={key} className="flex gap-2">
              <dt className="shrink-0 text-muted-foreground">{key}</dt>
              <dd className="min-w-0 whitespace-pre-wrap break-words">{value}</dd>
            </div>
          ))}
        </dl>
      ) : null}
      <div className="flex flex-wrap gap-2">
        <Button size="sm" disabled={answer.isPending} onClick={() => send(true)}>
          Allow
        </Button>
        <Button
          size="sm"
          variant="outline"
          disabled={answer.isPending}
          onClick={() => send(true, true)}
        >
          Allow for this run
        </Button>
        <Button size="sm" variant="outline" disabled={answer.isPending} onClick={() => send(false)}>
          Deny
        </Button>
      </div>
    </fieldset>
  );
}
