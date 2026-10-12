import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@circles/ui/components/dialog";
import { ACTION_GROUPS, flattenActions, useActionRegistry } from "@/lib/actions";
import { formatShortcut } from "@/lib/shortcuts";
import { useUi } from "@/stores/ui";

export function ShortcutsDialog() {
  const open = useUi((s) => s.shortcutsOpen);
  const setOpen = useUi((s) => s.setShortcutsOpen);
  const sources = useActionRegistry((s) => s.sources);
  const actions = flattenActions(sources).filter((a) => a.shortcuts?.length);

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="max-h-[80vh] overflow-y-auto sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Keyboard shortcuts</DialogTitle>
        </DialogHeader>
        <div className="flex flex-col gap-5">
          {ACTION_GROUPS.map((group) => {
            const items = actions.filter((a) => a.group === group);
            if (items.length === 0) return null;
            return (
              <section key={group} className="flex flex-col gap-1.5">
                <h3 className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
                  {group}
                </h3>
                <ul className="flex flex-col">
                  {items.map((action) => (
                    <li
                      key={action.id}
                      className="flex items-center justify-between gap-4 border-b border-border py-1.5 text-sm last:border-b-0"
                    >
                      <span>{action.title}</span>
                      <span className="flex gap-1">
                        {(action.shortcuts ?? []).map((s) => (
                          <kbd
                            key={formatShortcut(s)}
                            className="rounded border border-border bg-muted px-1.5 py-0.5 text-xs"
                          >
                            {formatShortcut(s)}
                          </kbd>
                        ))}
                      </span>
                    </li>
                  ))}
                </ul>
              </section>
            );
          })}
        </div>
      </DialogContent>
    </Dialog>
  );
}
