import { cn } from "@circles/ui/utils";
import {
  ChevronRightIcon,
  FileEditIcon,
  FilePlusIcon,
  FileTextIcon,
  GlobeIcon,
  Loader2Icon,
  type LucideIcon,
  SearchIcon,
  SparklesIcon,
  TerminalIcon,
  WrenchIcon,
} from "lucide-react";
import { createContext, type ReactNode, useContext, useState } from "react";
import type { ToolCall } from "./timeline";

const str = (value: unknown): string => (typeof value === "string" ? value : "");

/** Path shown relative to the workspace when it is inside it. */
function shortPath(path: string, root?: string): string {
  if (root && path.startsWith(`${root}/`)) return path.slice(root.length + 1);
  return path;
}

function Output({ text, error }: { text: string; error?: boolean }) {
  if (!text) return null;
  return (
    <pre
      className={cn(
        "max-h-60 overflow-auto whitespace-pre-wrap rounded-md bg-muted/60 p-2 font-mono text-[11px]",
        error && "text-destructive",
      )}
    >
      {text.length > 6000
        ? `${text.slice(0, 6000)}\n… ${text.length - 6000} more characters`
        : text}
    </pre>
  );
}

/** Removed and added lines of an edit, as a small diff. */
export function MiniDiff({ removed, added }: { removed: string; added: string }) {
  const lines = (text: string, sign: "-" | "+") =>
    text === ""
      ? []
      : text.split("\n").map((line, index) => ({ sign, line, key: `${sign}${index}` }));
  const rows = [...lines(removed, "-"), ...lines(added, "+")];
  return (
    <div className="max-h-60 overflow-auto rounded-md border border-border font-mono text-[11px]">
      {rows.map((row) => (
        <div
          key={row.key}
          className={cn(
            "whitespace-pre-wrap px-2",
            row.sign === "-" ? "bg-destructive/10 text-destructive" : "bg-success/10 text-success",
          )}
        >
          {row.sign} {row.line}
        </div>
      ))}
    </div>
  );
}

type Renderer = {
  icon: LucideIcon;
  /** The one-line description next to the tool name. */
  summary: (call: ToolCall, root?: string) => string;
  Body?: (props: { call: ToolCall; root?: string }) => ReactNode;
};

const REGISTRY: Record<string, Renderer> = {
  Read: {
    icon: FileTextIcon,
    summary: (c, root) => shortPath(str(c.input.file_path), root),
    Body: ({ call }) => <Output text={call.result ?? ""} error={call.isError} />,
  },
  Edit: {
    icon: FileEditIcon,
    summary: (c, root) => shortPath(str(c.input.file_path), root),
    Body: ({ call }) => (
      <>
        <MiniDiff removed={str(call.input.old_string)} added={str(call.input.new_string)} />
        {call.isError ? <Output text={call.result ?? ""} error /> : null}
      </>
    ),
  },
  MultiEdit: {
    icon: FileEditIcon,
    summary: (c, root) => shortPath(str(c.input.file_path), root),
    Body: ({ call }) => (
      <>
        {Array.isArray(call.input.edits)
          ? (call.input.edits as Record<string, unknown>[]).map((edit, index) => (
              <MiniDiff
                // biome-ignore lint/suspicious/noArrayIndexKey: edits have no id and never reorder
                key={index}
                removed={str(edit.old_string)}
                added={str(edit.new_string)}
              />
            ))
          : null}
        {call.isError ? <Output text={call.result ?? ""} error /> : null}
      </>
    ),
  },
  Write: {
    icon: FilePlusIcon,
    summary: (c, root) => shortPath(str(c.input.file_path), root),
    Body: ({ call }) => (
      <>
        <MiniDiff removed="" added={str(call.input.content)} />
        {call.isError ? <Output text={call.result ?? ""} error /> : null}
      </>
    ),
  },
  Bash: {
    icon: TerminalIcon,
    summary: (c) => str(c.input.description) || str(c.input.command).split("\n")[0],
    Body: ({ call }) => (
      <>
        <pre className="overflow-auto whitespace-pre-wrap rounded-md bg-muted/60 p-2 font-mono text-[11px]">
          <span className="text-muted-foreground">$ </span>
          {str(call.input.command)}
        </pre>
        <Output text={call.result ?? ""} error={call.isError} />
      </>
    ),
  },
  Grep: {
    icon: SearchIcon,
    summary: (c) => `${str(c.input.pattern)}${c.input.path ? ` in ${str(c.input.path)}` : ""}`,
    Body: ({ call }) => <Output text={call.result ?? ""} error={call.isError} />,
  },
  Glob: {
    icon: SearchIcon,
    summary: (c) => str(c.input.pattern),
    Body: ({ call }) => <Output text={call.result ?? ""} error={call.isError} />,
  },
  Task: {
    icon: SparklesIcon,
    summary: (c) => str(c.input.description) || str(c.input.subagent_type),
    Body: ({ call, root }) => (
      <>
        {str(call.input.prompt) ? (
          <p className="whitespace-pre-wrap text-xs text-muted-foreground">
            {str(call.input.prompt)}
          </p>
        ) : null}
        {call.children.length > 0 ? (
          <div className="flex flex-col gap-1 border-l border-border pl-3">
            {call.children.map((child) => (
              <ToolCallRow key={child.id} call={child} root={root} />
            ))}
          </div>
        ) : null}
        <Output text={call.result ?? ""} error={call.isError} />
      </>
    ),
  },
  WebFetch: {
    icon: GlobeIcon,
    summary: (c) => str(c.input.url),
    Body: ({ call }) => <Output text={call.result ?? ""} error={call.isError} />,
  },
  WebSearch: {
    icon: GlobeIcon,
    summary: (c) => str(c.input.query),
    Body: ({ call }) => <Output text={call.result ?? ""} error={call.isError} />,
  },
};

/** Tools without a renderer: the input as JSON and whatever came back. */
const GENERIC: Renderer = {
  icon: WrenchIcon,
  summary: (c) => {
    const first = Object.values(c.input).find((v) => typeof v === "string");
    return typeof first === "string" ? first.slice(0, 80) : "";
  },
  Body: ({ call }) => (
    <>
      <Output text={JSON.stringify(call.input, null, 2)} />
      <Output text={call.result ?? ""} error={call.isError} />
    </>
  ),
};

function rendererFor(name: string): Renderer {
  return REGISTRY[name] ?? GENERIC;
}

/** Set when the chat is not working, so a call that never got a result is not shown as running. */
export const ChatStoppedContext = createContext(false);

export function ToolCallRow({ call, root }: { call: ToolCall; root?: string }) {
  const [open, setOpen] = useState(false);
  const renderer = rendererFor(call.name);
  const Icon = renderer.icon;
  const Body = renderer.Body;
  const stopped = useContext(ChatStoppedContext);
  const running = call.result === undefined && !stopped;
  const label = call.name.startsWith("mcp__")
    ? call.name.split("__").slice(1).join(" · ")
    : call.name;
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
        <Icon className="size-3.5 shrink-0" />
        <span className="shrink-0 font-medium text-foreground">{label}</span>
        <span className="min-w-0 flex-1 truncate font-mono">{renderer.summary(call, root)}</span>
        {running ? (
          <Loader2Icon aria-label="Running" className="size-3 shrink-0 animate-spin" />
        ) : call.isError ? (
          <span className="shrink-0 text-destructive">failed</span>
        ) : null}
      </button>
      {open && Body ? (
        <div className="mt-1 mb-1 ml-5 flex flex-col gap-1.5">
          <Body call={call} root={root} />
        </div>
      ) : null}
    </div>
  );
}
