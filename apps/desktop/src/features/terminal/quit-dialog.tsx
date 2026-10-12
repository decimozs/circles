import { Button } from "@circles/ui/components/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@circles/ui/components/dialog";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { ipc } from "@/lib/ipc";

/**
 * Asked by the backend when the window is closed, or Cmd+Q is pressed, while terminals or scripts
 * are still running. Quitting ends all of them.
 */
export function QuitDialog() {
  const [running, setRunning] = useState<number | null>(null);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    listen<number>("app:confirm-quit", (event) => setRunning(event.payload))
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {
        // Not running inside Tauri: there is nothing to confirm.
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  const open = running !== null;
  const noun = running === 1 ? "process is" : "processes are";

  return (
    <Dialog open={open} onOpenChange={(o) => !o && setRunning(null)}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Quit Circles?</DialogTitle>
          <DialogDescription>
            {running} {noun} still running in your terminals, scripts and agent chats. Quitting
            stops
            {running === 1 ? " it" : " them"}.
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="outline" onClick={() => setRunning(null)}>
            Keep running
          </Button>
          <Button variant="destructive" onClick={() => ipc.appQuit().catch(() => {})}>
            Quit and stop everything
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
