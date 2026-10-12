import { Button } from "@circles/ui/components/button";
import { CornerDownLeftIcon, Loader2Icon, SquareIcon, XIcon } from "lucide-react";
import { ipc } from "@/lib/ipc";
import { useChat } from "@/stores/chat";
import { useSide } from "@/stores/side";
import { Markdown } from "./markdown";

/**
 * A side question and its answer, above the message box. Nothing here is part of the chat. It is
 * closed and forgotten, or the answer is brought into the message box as a quote.
 */
export function SidePane({ sessionId }: { sessionId: string }) {
  const item = useSide((s) => s.bySession[sessionId]);
  const close = useSide((s) => s.close);
  if (!item) return null;
  const running = item.status === "running";

  const stop = () => {
    ipc.sessionSideStop(sessionId).catch(() => {});
  };
  const closePane = () => {
    if (running) stop();
    close(sessionId);
  };
  const addToChat = () => {
    const quote = `> ${item.question.replace(/\n/g, "\n> ")}\n\n${item.answer}`;
    const draft = useChat.getState().drafts[sessionId] ?? "";
    useChat.getState().setDraft(sessionId, draft ? `${draft}\n\n${quote}` : quote);
    close(sessionId);
  };

  return (
    // The same side padding and width as the message box under it.
    <div className="mx-auto w-full max-w-3xl px-6 pb-2">
      <section
        aria-label="Side question"
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.stopPropagation();
            closePane();
          }
        }}
        className="flex max-h-72 w-full flex-col rounded-xl border border-border bg-card shadow-sm"
      >
        <header className="flex items-center gap-2 border-b border-border px-3 py-1.5 text-xs">
          <span className="font-medium">Side question</span>
          <span className="min-w-0 flex-1 truncate text-muted-foreground" title={item.question}>
            {item.question}
          </span>
          {running ? (
            <Button size="sm" variant="ghost" className="h-6 px-1.5 text-xs" onClick={stop}>
              <SquareIcon className="size-3" />
              Stop
            </Button>
          ) : null}
          <button
            type="button"
            aria-label="Close side question"
            onClick={closePane}
            className="flex size-6 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground"
          >
            <XIcon className="size-3.5" />
          </button>
        </header>
        <div className="min-h-0 flex-1 overflow-y-auto px-3 py-2 text-sm">
          {item.status === "failed" ? (
            <p role="alert" className="text-destructive">
              {item.error}
            </p>
          ) : item.answer ? (
            <Markdown text={item.answer} />
          ) : (
            <p className="flex items-center gap-2 text-muted-foreground">
              <Loader2Icon className="size-3.5 animate-spin" />
              Thinking…
            </p>
          )}
        </div>
        <footer className="flex items-center gap-2 border-t border-border px-3 py-1.5 text-xs text-muted-foreground">
          <span className="min-w-0 flex-1 truncate">
            {item.withContext
              ? "Not part of the chat. Answered from the conversation so far."
              : "Not part of the chat. This chat has not started, so there was no conversation to use."}
          </span>
          {item.status === "done" && item.answer ? (
            <Button size="sm" variant="outline" className="h-6 text-xs" onClick={addToChat}>
              <CornerDownLeftIcon className="size-3" />
              Add to chat
            </Button>
          ) : null}
        </footer>
      </section>
    </div>
  );
}
