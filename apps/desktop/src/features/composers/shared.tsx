import type { SharedComposer } from "@circles/bindings/SharedComposer";
import { Button } from "@circles/ui/components/button";
import { useQuery } from "@tanstack/react-query";
import { toast } from "sonner";
import { ipc } from "@/lib/ipc";
import { useComposerDialog } from "./store";

/** Composers kept in the repositories of your projects, for you to add to yours. */
export function SharedComposers() {
  const shared = useQuery({ queryKey: ["composer-shared"], queryFn: ipc.composerSharedList });
  const openCreate = useComposerDialog((s) => s.openCreate);
  const list = shared.data ?? [];
  if (list.length === 0) return null;

  const add = async (item: SharedComposer) => {
    try {
      openCreate(await ipc.composerSharedInput(item));
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <section aria-label="In your repositories" className="flex flex-col gap-2">
      <h2 className="text-sm font-medium">In your repositories</h2>
      <p className="text-xs text-muted-foreground">
        A shared file names a Composer's instructions, tools and limits. What it may do on its own
        is always your choice, so a new one starts by asking about everything.
      </p>
      <ul className="flex flex-col gap-2">
        {list.map((item) => (
          <li
            key={`${item.project_id}/${item.file}`}
            className="flex items-center justify-between gap-3 rounded-lg border border-border p-3"
          >
            <div className="min-w-0">
              <p className="truncate text-sm font-medium">{item.name}</p>
              <p className="truncate text-xs text-muted-foreground">
                {item.project_name} · {item.file}
              </p>
              {item.error ? (
                <p role="alert" className="text-xs text-destructive">
                  Cannot be used: {item.error}
                </p>
              ) : null}
            </div>
            <Button
              size="sm"
              variant="outline"
              disabled={!!item.error}
              onClick={() => void add(item)}
            >
              Add to mine
            </Button>
          </li>
        ))}
      </ul>
    </section>
  );
}
