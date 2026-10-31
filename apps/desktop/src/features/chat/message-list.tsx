import type { PermissionDecision } from "@circles/bindings/PermissionDecision";
import type { PermissionRequest } from "@circles/bindings/PermissionRequest";
import { cn } from "@circles/ui/utils";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ChevronDownIcon, Loader2Icon } from "lucide-react";
import {
  type ReactNode,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { AssistantText, PermissionCard, SystemNote, TurnSummary, UserBubble } from "./messages";
import type { TimelineItem } from "./timeline";
import { ChatStoppedContext } from "./tool-calls";
import type { Live } from "./use-session";
import { WorkBlock } from "./work-block";

/** Above this many rows only the visible ones are rendered. */
export const VIRTUALISE_FROM = 100;
const STICK_DISTANCE = 80;
/** Most ticks the map beside the chat shows. */
const MAX_TICKS = 70;
/** With fewer asks than this there is nothing to find one's way between. */
const MAP_FROM = 2;

type Row = { key: string; node: ReactNode; turnStart?: boolean };

/** What the turns of a chat can do, set when checkpoints exist. */
export type TurnActions = {
  /** Turns that can be gone back to, by turn number, with the message each starts with. */
  revertable: Map<number, { messageSeq: number }>;
  onRevert: (turn: number) => void;
  onBranch: (turn: number) => void;
};

function rowFor(item: TimelineItem, root?: string, turns?: TurnActions): Row {
  switch (item.type) {
    case "user":
      return {
        key: item.key,
        turnStart: (item.message.content as { steered?: boolean }).steered !== true,
        node: (
          <UserBubble
            item={item}
            onRevert={
              turns &&
              item.turn !== undefined &&
              turns.revertable.get(item.turn)?.messageSeq === item.message.seq
                ? () => turns.onRevert(item.turn as number)
                : undefined
            }
          />
        ),
      };
    case "assistant":
      return { key: item.key, node: <AssistantText text={item.text} /> };
    case "work":
      return { key: item.key, node: <WorkBlock id={item.key} steps={item.steps} root={root} /> };
    case "system":
      return { key: item.key, node: <SystemNote text={item.text} tone={item.tone} /> };
    case "summary":
      return {
        key: item.key,
        node: (
          <TurnSummary
            item={item}
            onBranch={
              turns?.revertable.has(item.turn) ? () => turns.onBranch(item.turn) : undefined
            }
          />
        ),
      };
  }
}

type Props = {
  items: TimelineItem[];
  live: Live;
  pending: PermissionRequest[];
  working: boolean;
  root?: string;
  onReply: (request: PermissionRequest, decision: PermissionDecision) => void;
  /** Shown above the first message. */
  header?: ReactNode;
  /** Reverting and branching, when the chat has checkpoints. */
  turns?: TurnActions;
};

export function MessageList({
  items,
  live,
  pending,
  working,
  root,
  onReply,
  header,
  turns,
}: Props) {
  const rows = useMemo<Row[]>(() => {
    const list: Row[] = items.map((item) => rowFor(item, root, turns));
    if (live.thinking) {
      // Thinking that streams right after earlier work joins that block.
      const last = items[items.length - 1];
      if (last?.type === "work") {
        list[list.length - 1] = {
          key: last.key,
          node: (
            <WorkBlock
              id={last.key}
              steps={last.steps}
              root={root}
              liveThinking={live.thinking}
              working
            />
          ),
        };
      } else {
        list.push({
          key: "live-work",
          node: <WorkBlock id="live-work" steps={[]} liveThinking={live.thinking} working />,
        });
      }
    }
    if (live.text) list.push({ key: "live-text", node: <AssistantText text={live.text} /> });
    for (const request of pending) {
      list.push({
        key: `permission-${request.request_id}`,
        node: <PermissionCard request={request} root={root} onReply={(d) => onReply(request, d)} />,
      });
    }
    if (working && pending.length === 0 && !live.text) {
      list.push({
        key: "working",
        node: (
          <div role="status" className="flex items-center gap-2 text-xs text-muted-foreground">
            <Loader2Icon className="size-3.5 animate-spin" />
            Working…
          </div>
        ),
      });
    }
    return list;
  }, [items, live, pending, working, root, onReply, turns]);

  const scroller = useRef<HTMLDivElement>(null);
  const content = useRef<HTMLDivElement>(null);
  const stick = useRef(true);
  const [atBottom, setAtBottom] = useState(true);
  const virtual = rows.length > VIRTUALISE_FROM;

  const virtualizer = useVirtualizer({
    count: virtual ? rows.length : 0,
    getScrollElement: () => scroller.current,
    estimateSize: () => 72,
    overscan: 8,
    getItemKey: (index) => rows[index]?.key ?? index,
    initialRect: { width: 800, height: 600 },
  });

  const scrollToBottom = useCallback(() => {
    const el = scroller.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, []);

  // Follow new content, unless the user scrolled up to read.
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs when the content changes
  useLayoutEffect(() => {
    if (stick.current) scrollToBottom();
  }, [rows, scrollToBottom]);

  // The content grows when the message box does, and when images and the like finish loading.
  // While the view is at the end it stays there.
  useEffect(() => {
    const el = content.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    const watcher = new ResizeObserver(() => {
      if (stick.current) scrollToBottom();
    });
    watcher.observe(el);
    return () => watcher.disconnect();
  }, [scrollToBottom]);

  const [current, setCurrent] = useState(0);

  /** The row in the middle of the view, or the last one when scrolled to the end. */
  const readCurrent = useCallback(
    (el: HTMLElement, near: boolean) => {
      if (near) return rows.length - 1;
      const middle = el.getBoundingClientRect().top + el.clientHeight / 2;
      let found = 0;
      for (const node of el.querySelectorAll<HTMLElement>("[data-index]")) {
        if (node.getBoundingClientRect().top <= middle) found = Number(node.dataset.index);
      }
      return found;
    },
    [rows.length],
  );

  const onScroll = () => {
    const el = scroller.current;
    if (!el) return;
    const near = el.scrollHeight - el.scrollTop - el.clientHeight < STICK_DISTANCE;
    stick.current = near;
    setAtBottom(near);
    setCurrent(readCurrent(el, near));
  };

  // New content moves the end of the list, so the marker follows it while the view sticks.
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs when the content changes
  useEffect(() => {
    const el = scroller.current;
    if (el) setCurrent(readCurrent(el, stick.current));
  }, [rows, readCurrent]);

  // A new turn from the user always brings the view back to the bottom.
  const userTurns = rows.filter((r) => r.turnStart).length;
  const seenTurns = useRef(userTurns);
  useEffect(() => {
    if (userTurns > seenTurns.current) {
      stick.current = true;
      setAtBottom(true);
      scrollToBottom();
    }
    seenTurns.current = userTurns;
  }, [userTurns, scrollToBottom]);

  // One tick for each thing the user asked, so the map tracks the asks and not every reply, tool
  // call and summary in between. Very many asks are thinned out evenly.
  const ticks = useMemo(() => {
    const asks = rows.flatMap((row, index) => (row.turnStart ? [index] : []));
    if (asks.length <= MAX_TICKS) return asks.map((index) => ({ index }));
    return Array.from({ length: MAX_TICKS }, (_, i) => ({
      index: asks[Math.floor((i * asks.length) / MAX_TICKS)] as number,
    }));
  }, [rows]);
  // The ask the view is in: the last one that starts at or above the visible row.
  const activeTick = Math.max(
    0,
    ticks.reduce((found, tick, i) => (tick.index <= current ? i : found), 0),
  );

  const jumpTo = (index: number) => {
    stick.current = false;
    setAtBottom(false);
    if (virtual) virtualizer.scrollToIndex(index, { align: "start" });
    else {
      const key = rows[index]?.key;
      scroller.current
        ?.querySelector(`[data-row-key="${CSS.escape(key ?? "")}"]`)
        ?.scrollIntoView({ block: "start" });
    }
  };

  return (
    <div className="relative min-h-0 flex-1">
      <div
        ref={scroller}
        onScroll={onScroll}
        className="h-full overflow-y-auto"
        data-testid="messages"
      >
        <ChatStoppedContext.Provider value={!working}>
          <div
            ref={content}
            // The message box floats over the end of the list, so the list leaves room for it.
            className="mx-auto flex max-w-3xl flex-col gap-3 px-6 pt-5 pb-[calc(var(--dock-height,0px)+1.25rem)]"
          >
            {header}
            {virtual ? (
              <div style={{ height: virtualizer.getTotalSize() }} className="relative">
                {virtualizer.getVirtualItems().map((v) => (
                  <div
                    key={v.key}
                    ref={virtualizer.measureElement}
                    data-index={v.index}
                    data-row-key={rows[v.index]?.key}
                    className="absolute inset-x-0 pb-3"
                    style={{ transform: `translateY(${v.start}px)` }}
                  >
                    {rows[v.index]?.node}
                  </div>
                ))}
              </div>
            ) : (
              rows.map((row) => (
                <div key={row.key} data-row-key={row.key} data-index={rows.indexOf(row)}>
                  {row.node}
                </div>
              ))
            )}
          </div>
        </ChatStoppedContext.Provider>
      </div>

      {ticks.length >= MAP_FROM ? (
        <nav
          aria-label="Conversation map"
          className="absolute top-1/2 right-0 flex w-6 -translate-y-1/2 flex-col items-end gap-[3px] py-2 pr-1.5"
        >
          {ticks.map((tick, i) => (
            <button
              key={tick.index}
              type="button"
              aria-label={`Go to question ${i + 1}`}
              aria-current={i === activeTick ? "location" : undefined}
              onClick={() => jumpTo(tick.index)}
              className="group flex h-[3px] w-6 items-center justify-end"
            >
              <span
                className={cn(
                  "h-px rounded-full transition-all",
                  i === activeTick
                    ? "w-4 bg-foreground"
                    : "w-3 bg-muted-foreground/70 group-hover:bg-foreground",
                )}
              />
            </button>
          ))}
        </nav>
      ) : null}

      {!atBottom ? (
        <button
          type="button"
          aria-label="Scroll to end"
          onClick={() => {
            stick.current = true;
            setAtBottom(true);
            scrollToBottom();
          }}
          className={cn(
            "glass-panel absolute shadow-md bottom-[calc(var(--dock-height,0px)+0.75rem)] left-1/2 z-20 flex h-7 -translate-x-1/2 items-center gap-1 rounded-full border px-3",
            "text-xs text-foreground hover:bg-accent",
          )}
        >
          <ChevronDownIcon className="size-3.5 text-muted-foreground" />
          Scroll to end
        </button>
      ) : null}
    </div>
  );
}
