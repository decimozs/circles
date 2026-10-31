import type { Composer } from "@circles/bindings/Composer";
import { cn } from "@circles/ui/utils";
import { ChevronDownIcon, ChevronRightIcon } from "lucide-react";
import { useState } from "react";
import { useApprovals } from "./api";
import { ApprovalCard, toolTitle } from "./approval-card";

/** Most answered requests listed. */
const ANSWERED = 5;

/** What waits for your answer, with the same buttons as the card in the chat, and what was answered
 * lately, folded away. It shows nothing for a run that never asked. */
export function ApprovalsSection({ composer }: { composer: Composer }) {
  const [showDone, setShowDone] = useState(false);
  const all = useApprovals({ composerId: composer.id }).data ?? [];
  const pending = all.filter((a) => a.status === "pending");
  const done = all.filter((a) => a.status !== "pending").slice(0, ANSWERED);
  if (all.length === 0) return null;
  return (
    <section aria-label="Approvals" className="flex flex-col border-t border-border py-1">
      <h2 className="flex items-baseline gap-1.5 px-3 pt-2 pb-1 text-xs text-muted-foreground">
        <span>Approvals</span>
        <span role="status" aria-live="polite">
          {pending.length > 0 ? `· ${pending.length} waiting` : "· nothing waiting"}
        </span>
      </h2>
      {pending.length > 0 ? (
        <div className="flex flex-col gap-2 px-3 pb-2">
          {pending.map((a) => (
            <ApprovalCard key={a.id} approval={a} />
          ))}
        </div>
      ) : null}
      {done.length > 0 ? (
        <>
          <button
            type="button"
            aria-expanded={showDone}
            onClick={() => setShowDone((v) => !v)}
            className="flex items-center gap-1 px-3 py-1.5 text-left text-xs text-muted-foreground hover:text-foreground"
          >
            {showDone ? (
              <ChevronDownIcon className="size-3" />
            ) : (
              <ChevronRightIcon className="size-3" />
            )}
            Answered {done.length}
          </button>
          {showDone ? (
            <ul aria-label="Answered" className="flex flex-col gap-1 px-3 pb-2 text-xs">
              {done.map((a) => (
                <li key={a.id} className="flex items-center justify-between gap-2">
                  <span className="truncate">{toolTitle(a.tool)}</span>
                  <span
                    className={cn(
                      "shrink-0 rounded px-1.5 py-0.5",
                      a.status === "allowed"
                        ? "bg-success/15 text-success"
                        : "bg-muted text-muted-foreground",
                    )}
                  >
                    {a.status}
                  </span>
                </li>
              ))}
            </ul>
          ) : null}
        </>
      ) : null}
    </section>
  );
}
