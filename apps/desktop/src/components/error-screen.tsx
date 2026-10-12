import { Button } from "@circles/ui/components/button";
import { useState } from "react";
import { ipc } from "@/lib/ipc";

/** What the person sees when a screen crashed: what happened, and a way out. */
export function ErrorScreen({
  error,
  title = "Something went wrong",
  onHome,
}: {
  error: Error;
  title?: string;
  /** Leave for the start screen. Offered where there is a router to go through. */
  onHome?: () => void;
}) {
  const [copied, setCopied] = useState(false);

  const copy = async () => {
    let report = "";
    try {
      report = await ipc.diagnosticsReport();
    } catch {
      // The report is a bonus. The error itself still gets copied.
    }
    const text = `${error.name}: ${error.message}\n${error.stack ?? ""}\n\n${report}`;
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
    } catch {
      // Clipboard blocked: nothing more to do.
    }
  };

  return (
    <div
      role="alert"
      className="flex h-full flex-col items-center justify-center gap-3 p-8 text-center"
    >
      <h1 className="text-lg font-semibold">{title}</h1>
      <p className="max-w-md text-sm text-muted-foreground">{error.message}</p>
      <div className="flex gap-2">
        {onHome ? <Button onClick={onHome}>Go to the start</Button> : null}
        <Button variant={onHome ? "outline" : "default"} onClick={() => window.location.reload()}>
          Reload
        </Button>
        <Button variant="outline" onClick={() => void copy()}>
          {copied ? "Copied" : "Copy diagnostics"}
        </Button>
      </div>
      <p className="text-xs text-muted-foreground">
        Your workspaces and chats are saved. Reloading does not stop running agents.
      </p>
    </div>
  );
}
