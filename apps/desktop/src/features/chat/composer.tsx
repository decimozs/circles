import type { Attachment } from "@circles/bindings/Attachment";
import type { Session } from "@circles/bindings/Session";
import { Button } from "@circles/ui/components/button";
import { cn } from "@circles/ui/utils";
import { useQuery } from "@tanstack/react-query";
import { ArrowUpIcon, ChevronsUpDownIcon, PaperclipIcon, SquareIcon, XIcon } from "lucide-react";
import { type ClipboardEvent, type KeyboardEvent, useLayoutEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { ClaudeIcon } from "@/components/claude-icon";
import { useSettingsView } from "@/features/settings/api";
import { ipc } from "@/lib/ipc";
import { MODELS } from "@/lib/models";
import { type QueuedMessage, useChat } from "@/stores/chat";
import { useSlashCommands } from "./api";
import { EffortPicker } from "./effort-picker";
import { AttachmentChip } from "./messages";

/** Pasted text longer than this becomes an attachment instead of filling the box. */
export const LONG_PASTE = 5000;

const selectClass =
  "h-7 rounded-md border border-transparent bg-transparent px-1.5 text-xs text-muted-foreground hover:bg-muted focus-visible:border-ring focus-visible:outline-none";

/** Text style shared by the box and the layer behind it, so the badges line up with the words. */
const TEXT = "block whitespace-pre-wrap break-words px-3 py-2.5 text-sm leading-normal";

export type Segment = { text: string; start: number; kind?: "mention" | "command" };

/**
 * Splits the message into plain text and the `@file` and `/command` tokens in it, so the composer
 * can paint a badge behind them. A command counts only at the start of the message.
 */
export function highlight(text: string): Segment[] {
  const found: { start: number; end: number; kind: "mention" | "command" }[] = [];
  for (const match of text.matchAll(/(^|\s)(@[^\s@]+)|^(\/[^\s/]+)/g)) {
    if (match[3]) {
      found.push({ start: match.index, end: match.index + match[3].length, kind: "command" });
    } else {
      const start = match.index + match[1].length;
      found.push({ start, end: start + match[2].length, kind: "mention" });
    }
  }
  const segments: Segment[] = [];
  let at = 0;
  for (const token of found) {
    if (token.start > at) segments.push({ text: text.slice(at, token.start), start: at });
    segments.push({
      text: text.slice(token.start, token.end),
      start: token.start,
      kind: token.kind,
    });
    at = token.end;
  }
  if (at < text.length) segments.push({ text: text.slice(at), start: at });
  return segments;
}

type Trigger =
  | { kind: "mention"; query: string; start: number }
  | { kind: "slash"; query: string; start: number }
  | null;

/** The `@file` or `/command` the caret is in, if any. */
export function findTrigger(text: string, caret: number): Trigger {
  const before = text.slice(0, caret);
  const mention = /(^|\s)@([^\s@]*)$/.exec(before);
  if (mention) {
    return { kind: "mention", query: mention[2], start: caret - mention[2].length - 1 };
  }
  const slash = /^\/([^\s/]*)$/.exec(before);
  if (slash) return { kind: "slash", query: slash[1], start: 0 };
  return null;
}

function toBase64(file: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result).split(",")[1] ?? "");
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(file);
  });
}

type Props = {
  workspaceId: string;
  session: Session;
  working: boolean;
  disabled?: boolean;
  queue: QueuedMessage[];
  onSend: (text: string, attachments: Attachment[], how: "send" | "queue") => void;
  onStop: () => void;
  onChangeOptions: (options: { model?: string; effort?: string; permissionMode?: string }) => void;
  /** A side question: answered in a pane, and not part of the chat. */
  onSide?: (question: string) => void;
  /** A likely next message from a model. It is the placeholder of an empty box, and Tab takes it. */
  suggestion?: string | null;
  /** A Composer's run has no permission mode: its tools ask for themselves. */
  variant?: "workspace" | "composer";
  /** For a Composer's run: the Composer's mode, which is what its run may do without asking. */
  modeControl?: { value: string; onChange: (mode: string) => void };
};

export function Composer({
  workspaceId,
  session,
  working,
  disabled,
  queue,
  onSend,
  onStop,
  onChangeOptions,
  variant = "workspace",
  modeControl,
  suggestion,
  onSide,
}: Props) {
  const mode =
    variant === "composer"
      ? modeControl
      : {
          value: session.permission_mode as string,
          onChange: (value: string) => onChangeOptions({ permissionMode: value }),
        };
  const { sendKey, followUp, drafts, setDraft, updateQueued, removeQueued } = useChat();
  const longAsFile = useSettingsView().data?.settings.general.long_text_to_attachment !== false;
  const text = drafts[session.id] ?? "";
  const [attachments, setAttachments] = useState<Attachment[]>([]);
  const [caret, setCaret] = useState(0);
  const [picked, setPicked] = useState(0);
  const [dismissed, setDismissed] = useState<string | null>(null);
  const [editing, setEditing] = useState<string | null>(null);
  const box = useRef<HTMLTextAreaElement>(null);
  const marks = useRef<HTMLDivElement>(null);
  const pendingCaret = useRef<number | null>(null);
  const segments = highlight(text);
  const files = useRef<HTMLInputElement>(null);

  const trigger = findTrigger(text, caret);
  const triggerId = trigger ? `${trigger.kind}:${trigger.start}` : null;
  const open = trigger !== null && dismissed !== triggerId;

  const hits = useQuery({
    queryKey: ["mention", workspaceId, trigger?.kind === "mention" ? trigger.query : ""],
    queryFn: () => ipc.searchFiles(workspaceId, (trigger as { query: string }).query, 8),
    enabled: open && trigger?.kind === "mention",
    placeholderData: (previous) => previous,
  });
  const commands = useSlashCommands(workspaceId);

  const suggestions: { value: string; label: string; hint?: string }[] = !open
    ? []
    : trigger?.kind === "mention"
      ? [
          ...(hits.data ?? []).map((h) => ({
            value: `@${h.path}${h.is_dir ? "/" : ""}`,
            label: h.path + (h.is_dir ? "/" : ""),
          })),
        ]
      : (commands.data ?? [])
          .filter((c) => c.name.toLowerCase().includes((trigger?.query ?? "").toLowerCase()))
          .slice(0, 8)
          .map((c) => ({ value: `/${c.name}`, label: `/${c.name}`, hint: c.description }));

  const accept = (index: number) => {
    const choice = suggestions[index];
    if (!choice || !trigger) return;
    const next = `${text.slice(0, trigger.start)}${choice.value} ${text.slice(caret)}`;
    setDraft(session.id, next);
    setPicked(0);
    pendingCaret.current = trigger.start + choice.value.length + 1;
  };

  // The caret moves to after the inserted text as soon as it is in the box, so what the user types
  // next never lands before it.
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs when the text changes
  useLayoutEffect(() => {
    const position = pendingCaret.current;
    if (position === null || !box.current) return;
    pendingCaret.current = null;
    box.current.focus();
    box.current.setSelectionRange(position, position);
    setCaret(position);
  }, [text]);

  const attach = async (blob: Blob, name: string) => {
    try {
      const saved = await ipc.attachmentSave(workspaceId, name, await toBase64(blob));
      setAttachments((list) => [...list, saved]);
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    }
  };

  const submit = (how: "send" | "queue") => {
    // `/btw question` is a side question. It goes to the pane, and the chat does not hear of it.
    const side = /^\/btw\s+([\s\S]+)$/.exec(text.trim());
    if (side && onSide && !disabled) {
      onSide(side[1].trim());
      setDraft(session.id, "");
      return;
    }
    if (disabled || (!text.trim() && attachments.length === 0)) return;
    onSend(text.trim(), attachments, how);
    setDraft(session.id, "");
    setAttachments([]);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (suggestions.length > 0) {
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        const step = event.key === "ArrowDown" ? 1 : suggestions.length - 1;
        setPicked((i) => (i + step) % suggestions.length);
        return;
      }
      if (event.key === "Enter" || event.key === "Tab") {
        event.preventDefault();
        accept(picked);
        return;
      }
    }
    // ⌘⇧B starts a side question.
    if (event.key.toLowerCase() === "b" && event.shiftKey && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      setDraft(session.id, "/btw ");
      pendingCaret.current = 5;
      return;
    }
    // Tab takes the suggestion, only into an empty box and only when no list is open.
    if (event.key === "Tab" && !event.shiftKey && suggestion && !text && !working) {
      event.preventDefault();
      setDraft(session.id, suggestion);
      pendingCaret.current = suggestion.length;
      return;
    }
    if (open && event.key === "Escape") {
      event.preventDefault();
      setDismissed(triggerId);
      return;
    }
    if (event.key !== "Enter" || event.shiftKey || event.nativeEvent.isComposing) return;
    const mod = event.metaKey || event.ctrlKey;
    const wantsSend = sendKey === "enter" ? true : mod;
    if (!wantsSend) return;
    event.preventDefault();
    if (!working) return submit("send");
    // While working, Enter does what the setting says and Cmd+Enter does the other.
    const usual = followUp === "steer" ? "send" : "queue";
    const opposite = followUp === "steer" ? "queue" : "send";
    submit(sendKey === "enter" && mod ? opposite : usual);
  };

  const onPaste = (event: ClipboardEvent<HTMLTextAreaElement>) => {
    const pasted = [...event.clipboardData.files];
    if (pasted.length > 0) {
      event.preventDefault();
      for (const file of pasted) void attach(file, file.name || "pasted-file");
      return;
    }
    const long = event.clipboardData.getData("text");
    if (longAsFile && long.length > LONG_PASTE) {
      event.preventDefault();
      void attach(new Blob([long], { type: "text/plain" }), "pasted-text.txt");
    }
  };

  const canSend = text.trim().length > 0 || attachments.length > 0;

  return (
    <div className="mx-auto w-full max-w-3xl px-6 pb-4">
      {queue.length > 0 ? (
        <ul aria-label="Queued messages" className="mb-2 flex flex-col gap-1">
          {queue.map((item) => (
            <li
              key={item.id}
              className="flex items-center gap-2 rounded-lg border border-dashed border-border px-2.5 py-1.5 text-xs"
            >
              <span className="shrink-0 text-muted-foreground">Queued</span>
              {editing === item.id ? (
                <input
                  // biome-ignore lint/a11y/noAutofocus: focus moves to the field the user just opened
                  autoFocus
                  aria-label="Edit queued message"
                  defaultValue={item.text}
                  className="min-w-0 flex-1 bg-transparent outline-none"
                  onBlur={(e) => {
                    updateQueued(session.id, item.id, e.target.value);
                    setEditing(null);
                  }}
                  onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
                />
              ) : (
                <button
                  type="button"
                  className="min-w-0 flex-1 truncate text-left"
                  onClick={() => setEditing(item.id)}
                >
                  {item.text || `${item.attachments.length} attachment(s)`}
                </button>
              )}
              <button
                type="button"
                aria-label="Cancel queued message"
                className="text-muted-foreground hover:text-foreground"
                onClick={() => removeQueued(session.id, item.id)}
              >
                <XIcon className="size-3.5" />
              </button>
            </li>
          ))}
        </ul>
      ) : null}

      <div className="relative rounded-xl border border-input bg-card focus-within:border-ring">
        {suggestions.length > 0 ? (
          <div
            role="listbox"
            aria-label={trigger?.kind === "mention" ? "Files" : "Commands"}
            className="absolute right-0 bottom-full left-0 mb-1 max-h-56 overflow-auto rounded-lg border border-border bg-popover p-1 shadow-md"
          >
            {suggestions.map((s, index) => (
              <div
                key={s.value}
                role="option"
                tabIndex={-1}
                aria-selected={index === picked}
                onMouseDown={(e) => {
                  e.preventDefault();
                  accept(index);
                }}
                className={cn(
                  "flex w-full cursor-default items-baseline gap-2 rounded-md px-2 py-1 text-left text-xs",
                  index === picked && "bg-muted",
                )}
              >
                <span className="truncate font-mono">{s.label}</span>
                {s.hint ? <span className="truncate text-muted-foreground">{s.hint}</span> : null}
              </div>
            ))}
          </div>
        ) : null}

        {attachments.length > 0 ? (
          <div className="flex flex-wrap gap-1 px-3 pt-2">
            {attachments.map((a) => (
              <span
                key={a.path}
                className="inline-flex items-center gap-1 rounded-md bg-muted pr-1"
              >
                <AttachmentChip attachment={a} />
                <button
                  type="button"
                  aria-label={`Remove ${a.name}`}
                  onClick={() => setAttachments((list) => list.filter((x) => x.path !== a.path))}
                >
                  <XIcon className="size-3" />
                </button>
              </span>
            ))}
          </div>
        ) : null}

        <div className="relative">
          <div
            ref={marks}
            aria-hidden
            data-testid="composer-marks"
            className={cn(
              TEXT,
              "pointer-events-none absolute inset-0 overflow-hidden text-transparent",
            )}
          >
            {segments.map((segment) => (
              <span
                key={segment.start}
                data-kind={segment.kind}
                className={cn(
                  segment.kind === "mention" && "rounded-sm bg-info/20 ring-1 ring-info/40",
                  segment.kind === "command" && "rounded-sm bg-success/20 ring-1 ring-success/40",
                )}
              >
                {segment.text}
              </span>
            ))}
            {"\n"}
          </div>
          <textarea
            ref={box}
            aria-label="Message"
            rows={2}
            disabled={disabled}
            value={text}
            placeholder={
              working
                ? "Steer the agent, or queue a follow-up…"
                : suggestion
                  ? `${suggestion}  (Tab)`
                  : variant === "composer"
                    ? "Tell the Composer what you want done"
                    : "Message the agent. @ for files, / for commands"
            }
            onChange={(e) => {
              setDraft(session.id, e.target.value);
              setCaret(e.target.selectionStart);
              setPicked(0);
            }}
            onSelect={(e) => setCaret(e.currentTarget.selectionStart)}
            onKeyDown={onKeyDown}
            onPaste={onPaste}
            className={cn(
              TEXT,
              "relative max-h-60 min-h-14 w-full resize-none bg-transparent outline-none placeholder:text-muted-foreground disabled:opacity-50",
            )}
            onScroll={(e) => {
              if (marks.current) marks.current.scrollTop = e.currentTarget.scrollTop;
            }}
          />
        </div>

        <div className="flex items-center gap-1 px-2 pb-2">
          <input
            ref={files}
            type="file"
            multiple
            hidden
            aria-label="Attach files"
            onChange={(e) => {
              for (const file of e.target.files ?? []) void attach(file, file.name);
              e.target.value = "";
            }}
          />
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            aria-label="Attach a file"
            onClick={() => files.current?.click()}
          >
            <PaperclipIcon />
          </Button>
          {mode ? (
            <div className="relative flex items-center">
              <select
                aria-label={variant === "composer" ? "Mode" : "Permission mode"}
                title={
                  variant === "composer"
                    ? "What this Composer may do without asking"
                    : "What the agent may do without asking"
                }
                className={cn(
                  selectClass,
                  "appearance-none pr-6",
                  mode.value === "bypass" && "text-destructive",
                )}
                value={mode.value}
                onChange={(e) => mode.onChange(e.target.value)}
              >
                <option value="manual">Ask</option>
                <option value="accept_edits">Accept edits</option>
                <option value="plan">Plan</option>
                <option value="bypass">Bypass</option>
              </select>
              {/* Drawn here, not by the select, so only the words take the warning colour. */}
              <ChevronsUpDownIcon className="pointer-events-none absolute right-1.5 size-3.5 text-muted-foreground" />
            </div>
          ) : null}
          <div className="ml-auto flex items-center gap-1">
            <div className="relative flex items-center">
              <ClaudeIcon className="pointer-events-none absolute -left-4" />
              <select
                aria-label="Model"
                className={selectClass}
                style={{ paddingLeft: 30 }}
                value={session.model ?? ""}
                onChange={(e) => onChangeOptions({ model: e.target.value })}
              >
                {session.model && !MODELS.some((m) => m.id === session.model) ? (
                  <option value={session.model}>{session.model}</option>
                ) : null}
                {MODELS.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.label}
                  </option>
                ))}
              </select>
            </div>
            <EffortPicker
              value={session.effort}
              onChange={(effort) => onChangeOptions({ effort })}
            />
            {working ? (
              <Button
                type="button"
                size="icon-sm"
                variant="outline"
                className="rounded-full"
                aria-label="Stop"
                onClick={onStop}
              >
                <SquareIcon />
              </Button>
            ) : null}
            {!working || canSend ? (
              <Button
                type="button"
                size="icon-sm"
                className="rounded-full"
                aria-label={
                  working ? (followUp === "queue" ? "Queue message" : "Send to steer") : "Send"
                }
                disabled={disabled || !canSend}
                onClick={() => submit(working && followUp === "queue" ? "queue" : "send")}
              >
                <ArrowUpIcon />
              </Button>
            ) : null}
          </div>
        </div>
      </div>
    </div>
  );
}
