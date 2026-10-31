import { cn } from "@circles/ui/utils";
import { BrainIcon, ChevronRightIcon, Loader2Icon } from "lucide-react";
import { useContext, useState } from "react";
import type { WorkStep } from "./timeline";
import { ChatStoppedContext, ToolCallRow } from "./tool-calls";

/** Names for a stretch of work. One is picked for each block and stays the same for it. */
export const WORK_LABELS = [
  "Thinking",
  "Processing",
  "Pondering",
  "Reasoning",
  "Mulling it over",
  "Working it out",
  "Digging in",
  "Connecting the dots",
];

export function workLabel(key: string): string {
  let hash = 0;
  for (const char of key) hash = (hash * 31 + char.charCodeAt(0)) >>> 0;
  return WORK_LABELS[hash % WORK_LABELS.length];
}

type Props = {
  id: string;
  steps: WorkStep[];
  root?: string;
  /** Thinking that is still being written and not stored yet. */
  liveThinking?: string;
  /** Claude is still working, so the block shows a spinner. */
  working?: boolean;
};

/** Thinking and tool calls of one stretch of work, behind a single collapsed line. */
export function WorkBlock({ id, steps, root, liveThinking, working }: Props) {
  const [open, setOpen] = useState(false);
  const calls = steps.filter((s) => s.kind === "tool");
  const stopped = useContext(ChatStoppedContext);
  const running =
    working || (!stopped && calls.some((s) => s.call.result === undefined)) || !!liveThinking;
  const failed = calls.filter((s) => s.call.isError).length;
  const label = workLabel(id);

  return (
    <div className="text-xs">
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
        className="flex w-fit max-w-full items-center gap-1.5 rounded-md px-1.5 py-1 text-left text-muted-foreground hover:bg-muted/60"
      >
        <ChevronRightIcon
          className={cn("size-3 shrink-0 transition-transform", open && "rotate-90")}
        />
        <BrainIcon className="size-3.5 shrink-0" />
        <span>{running ? `${label}…` : label}</span>
        {calls.length > 0 ? (
          <span>
            · {calls.length} tool {calls.length === 1 ? "call" : "calls"}
          </span>
        ) : null}
        {failed > 0 ? <span className="text-destructive">{failed} failed</span> : null}
        {running ? <Loader2Icon aria-label="Running" className="size-3 animate-spin" /> : null}
      </button>
      {open ? (
        <div className="mt-1 ml-3 flex flex-col gap-1 border-l border-border pl-3">
          {steps.map((step) =>
            step.kind === "thinking" ? (
              <p key={step.key} className="whitespace-pre-wrap italic text-muted-foreground">
                {step.text || "The agent's reasoning is not shown for this step."}
              </p>
            ) : (
              <ToolCallRow key={step.key} call={step.call} root={root} />
            ),
          )}
          {liveThinking ? (
            <p className="whitespace-pre-wrap italic text-muted-foreground">{liveThinking}</p>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
