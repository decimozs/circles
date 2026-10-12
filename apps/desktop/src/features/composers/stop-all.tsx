import { Button } from "@circles/ui/components/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@circles/ui/components/dialog";
import { OctagonXIcon } from "lucide-react";
import { toast } from "sonner";
import { create } from "zustand";
import { usePause, useStopAll } from "./api";

/** Opens the stop-everything question from anywhere, such as the keyboard shortcut. */
export const useStopAllDialog = create<{ open: boolean; setOpen: (open: boolean) => void }>(
  (set) => ({ open: false, setOpen: (open) => set({ open }) }),
);

/** The kill switch: pauses every Composer, stops their runs and the agents they started. */
export function StopAllDialog() {
  const open = useStopAllDialog((s) => s.open);
  const setOpen = useStopAllDialog((s) => s.setOpen);
  const stop = useStopAll();
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Stop every Composer?</DialogTitle>
          <DialogDescription>
            This stops all Composer runs and the agents they started, says no to everything that is
            waiting for you, and pauses Composers so nothing starts again until you resume them.
            Work already on disk stays.
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="outline" onClick={() => setOpen(false)}>
            Cancel
          </Button>
          <Button
            variant="destructive"
            disabled={stop.isPending}
            onClick={() =>
              stop.mutate(undefined, {
                onSuccess: () => {
                  setOpen(false);
                  toast("Composers are stopped and paused");
                },
                onError: (e) => toast.error(e instanceof Error ? e.message : String(e)),
              })
            }
          >
            <OctagonXIcon />
            Stop everything
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

/** The pause switch: while paused, no tool call of any Composer goes through. */
export function PauseButton({ paused }: { paused: boolean }) {
  const pause = usePause();
  return (
    <Button
      size="sm"
      variant={paused ? "default" : "outline"}
      disabled={pause.isPending}
      onClick={() =>
        pause.mutate(!paused, {
          onError: (e) => toast.error(e instanceof Error ? e.message : String(e)),
        })
      }
    >
      {paused ? "Resume Composers" : "Pause Composers"}
    </Button>
  );
}

export function StopAllButton() {
  const setOpen = useStopAllDialog((s) => s.setOpen);
  return (
    <Button size="sm" variant="outline" onClick={() => setOpen(true)}>
      <OctagonXIcon />
      Stop all
    </Button>
  );
}
