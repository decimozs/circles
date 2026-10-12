import { cn } from "@circles/ui/utils";
import { type KeyboardEvent, useLayoutEffect, useRef, useState } from "react";
import { useComposerSkills } from "./api";

/** Text style shared by the box and the layer behind it, so the colour lines up with the words. */
const TEXT = "block whitespace-pre-wrap break-words px-2.5 py-2 text-sm leading-normal";

export type Segment = { text: string; start: number; skill?: boolean };

/**
 * Splits the text into plain text and the `/skill` words that name one of the user's skills, so the
 * box can colour them the way the chat input colours a command. A word that is not a skill, or
 * that is part of a path such as src/lib, stays plain.
 */
export function highlightSkills(text: string, known: ReadonlySet<string>): Segment[] {
  const found: { start: number; end: number }[] = [];
  for (const match of text.matchAll(/(^|\s)\/([A-Za-z0-9_.:-]+)/g)) {
    const name = match[2].replace(/[.:]+$/, "");
    if (!known.has(name)) continue;
    const start = match.index + match[1].length;
    found.push({ start, end: start + 1 + name.length });
  }
  const segments: Segment[] = [];
  let at = 0;
  for (const token of found) {
    if (token.start > at) segments.push({ text: text.slice(at, token.start), start: at });
    segments.push({ text: text.slice(token.start, token.end), start: token.start, skill: true });
    at = token.end;
  }
  if (at < text.length) segments.push({ text: text.slice(at), start: at });
  return segments;
}

/** The `/name` the caret is in, if it starts a word. */
export function findSlash(text: string, caret: number): { query: string; start: number } | null {
  const match = /(^|\s)\/([^\s/]*)$/.exec(text.slice(0, caret));
  return match ? { query: match[2], start: caret - match[2].length - 1 } : null;
}

const COPIED = [
  "boxSizing",
  "width",
  "paddingTop",
  "paddingRight",
  "paddingBottom",
  "paddingLeft",
  "borderTopWidth",
  "borderRightWidth",
  "borderBottomWidth",
  "borderLeftWidth",
  "fontFamily",
  "fontSize",
  "fontWeight",
  "letterSpacing",
  "lineHeight",
  "textTransform",
  "wordSpacing",
  "textIndent",
  "whiteSpace",
  "overflowWrap",
  "wordBreak",
  "tabSize",
] as const;

/** Where the line the caret is on ends, in pixels from the top of the box. */
function belowCaret(box: HTMLTextAreaElement, caret: number): number {
  const style = getComputedStyle(box);
  const mirror = document.createElement("div");
  for (const key of COPIED) mirror.style[key] = style[key];
  mirror.style.position = "absolute";
  mirror.style.visibility = "hidden";
  mirror.style.whiteSpace = "pre-wrap";
  mirror.textContent = box.value.slice(0, caret);
  const mark = document.createElement("span");
  mark.textContent = "\u200b";
  mirror.appendChild(mark);
  document.body.appendChild(mirror);
  const line = Number.parseFloat(style.lineHeight) || Number.parseFloat(style.fontSize) * 1.4;
  const top = mark.offsetTop + line - box.scrollTop;
  mirror.remove();
  return Math.min(Math.max(top, line), box.clientHeight);
}

/**
 * A text box where `/` offers the user's skills, wherever it starts a word. A Composer cannot run a
 * skill by itself, so Circles puts what the skill says in front of it when a run starts.
 */
export function SkillTextarea({
  value,
  onValueChange,
  className,
  ...props
}: Omit<React.ComponentProps<"textarea">, "onChange" | "value"> & {
  value: string;
  onValueChange: (value: string) => void;
}) {
  const skills = useComposerSkills().data ?? [];
  const known = new Set(skills.map((s) => s.name));
  const segments = highlightSkills(value, known);
  const marks = useRef<HTMLDivElement>(null);
  const box = useRef<HTMLTextAreaElement>(null);
  const pendingCaret = useRef<number | null>(null);
  const [caret, setCaret] = useState(0);
  const [picked, setPicked] = useState(0);
  const [dismissed, setDismissed] = useState<number | null>(null);

  const slash = findSlash(value, caret);
  const [top, setTop] = useState(0);
  const open = slash !== null && dismissed !== slash.start;
  const suggestions = open
    ? skills.filter((s) => s.name.toLowerCase().includes(slash.query.toLowerCase())).slice(0, 8)
    : [];

  const accept = (index: number) => {
    const choice = suggestions[index];
    if (!choice || !slash) return;
    onValueChange(`${value.slice(0, slash.start)}/${choice.name} ${value.slice(caret)}`);
    pendingCaret.current = slash.start + choice.name.length + 2;
    setPicked(0);
  };

  // The list sits under the line being typed, not under the whole box.
  // biome-ignore lint/correctness/useExhaustiveDependencies: follows the text and the caret
  useLayoutEffect(() => {
    if (open && box.current) setTop(belowCaret(box.current, caret));
  }, [open, value, caret]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: runs when the text changes
  useLayoutEffect(() => {
    const position = pendingCaret.current;
    if (position === null || !box.current) return;
    pendingCaret.current = null;
    box.current.focus();
    box.current.setSelectionRange(position, position);
    setCaret(position);
  }, [value]);

  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (suggestions.length === 0) return;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const step = event.key === "ArrowDown" ? 1 : suggestions.length - 1;
      setPicked((i) => (i + step) % suggestions.length);
    } else if (event.key === "Enter" || event.key === "Tab") {
      event.preventDefault();
      accept(picked);
    } else if (event.key === "Escape") {
      // Closes the list, not the dialog around it.
      event.preventDefault();
      event.stopPropagation();
      setDismissed(slash?.start ?? null);
    }
  };

  return (
    <div className="relative">
      <div className="relative rounded-lg border border-input bg-transparent transition-colors focus-within:border-ring focus-within:ring-3 focus-within:ring-ring/50 dark:bg-input/30">
        <div
          ref={marks}
          aria-hidden
          data-testid="skill-marks"
          className={cn(
            TEXT,
            "pointer-events-none absolute inset-0 overflow-hidden text-transparent",
          )}
        >
          {segments.map((segment) => (
            <span
              key={segment.start}
              data-skill={segment.skill ? "true" : undefined}
              className={cn(segment.skill && "rounded-sm bg-success/20 ring-1 ring-success/40")}
            >
              {segment.text}
            </span>
          ))}
          {"\n"}
        </div>
        <textarea
          {...props}
          ref={box}
          rows={9}
          value={value}
          className={cn(
            TEXT,
            "relative min-h-44 max-h-72 w-full resize-y bg-transparent outline-none placeholder:text-muted-foreground",
            className,
          )}
          onChange={(e) => {
            onValueChange(e.target.value);
            setCaret(e.target.selectionStart);
          }}
          onSelect={(e) => setCaret(e.currentTarget.selectionStart)}
          onScroll={(e) => {
            if (marks.current) marks.current.scrollTop = e.currentTarget.scrollTop;
          }}
          onKeyDown={onKeyDown}
        />
      </div>
      {suggestions.length > 0 ? (
        <div
          role="listbox"
          aria-label="Skills"
          style={{ top }}
          className="absolute right-0 left-0 z-50 mt-1 max-h-36 overflow-auto rounded-lg border border-border bg-popover p-1 shadow-md"
        >
          {suggestions.map((s, index) => (
            <div
              key={s.name}
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
              <span className="shrink-0 font-mono">/{s.name}</span>
              <span className="truncate text-muted-foreground">{s.description}</span>
            </div>
          ))}
        </div>
      ) : null}
    </div>
  );
}
