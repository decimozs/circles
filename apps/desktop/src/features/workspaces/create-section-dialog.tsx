import { Button } from "@circles/ui/components/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@circles/ui/components/dialog";
import { Input } from "@circles/ui/components/input";
import { useEffect, useState } from "react";
import { toast } from "sonner";
import { ErrorMessage } from "@/features/projects/error-message";
import { useCreateSection } from "@/features/workspaces/api";
import { useUi } from "@/stores/ui";

export function CreateSectionDialog() {
  const isOpen = useUi((s) => s.createSectionOpen);
  const setOpen = useUi((s) => s.setCreateSectionOpen);
  const create = useCreateSection();
  const [name, setName] = useState("");

  // biome-ignore lint/correctness/useExhaustiveDependencies: reset only when it opens
  useEffect(() => {
    if (isOpen) {
      setName("");
      create.reset();
    }
  }, [isOpen]);

  return (
    <Dialog open={isOpen} onOpenChange={(o) => setOpen(o)}>
      <DialogContent className="sm:max-w-sm">
        <form
          className="flex flex-col gap-4"
          onSubmit={(e) => {
            e.preventDefault();
            create.mutate(name, {
              onSuccess: (section) => {
                setOpen(false);
                toast.success(`Created section ${section.name}`, {
                  description: "Assign workspaces to it from the Diagnostics page for now.",
                });
              },
            });
          }}
        >
          <DialogHeader>
            <DialogTitle>Create section</DialogTitle>
            <DialogDescription>
              Sections group workspaces together in the sidebar.
            </DialogDescription>
          </DialogHeader>
          <Input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="Section name"
            aria-label="Section name"
            autoFocus
          />
          <ErrorMessage error={create.error} />
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => setOpen(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={!name.trim() || create.isPending}>
              Create
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
