import { Button } from "@circles/ui/components/button";
import { Input } from "@circles/ui/components/input";
import { DownloadIcon } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { ipc } from "@/lib/ipc";
import { useAudit, useComposers } from "./api";
import { AuditRow } from "./composer-panel";

/** Every tool call every Composer made, with who allowed it and what came of it. */
export function AuditPage() {
  const composers = useComposers().data ?? [];
  const [composerId, setComposerId] = useState("");
  const [search, setSearch] = useState("");
  const entries = useAudit({ composerId: composerId || undefined, search }).data ?? [];
  const names = new Map(composers.map((c) => [c.id, c.name]));

  const exportCsv = async () => {
    try {
      const path = await ipc.auditExport(composerId || undefined);
      toast(`Exported to ${path}`);
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-4 px-6 py-8">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-xl font-semibold">Audit log</h1>
          <p className="text-sm text-muted-foreground">
            Every tool call a Composer made, allowed or not. Secrets are removed before they are
            kept.
          </p>
        </div>
        <Button size="sm" variant="outline" onClick={() => void exportCsv()}>
          <DownloadIcon />
          Export CSV
        </Button>
      </header>
      <div className="flex gap-2">
        <Input
          aria-label="Search the audit log"
          placeholder="Search tool, input or result"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          className="h-8 text-xs"
        />
        <select
          aria-label="Composer"
          value={composerId}
          onChange={(e) => setComposerId(e.target.value)}
          className="h-8 rounded-md border border-input bg-background px-2 text-xs"
        >
          <option value="">All Composers</option>
          {composers.map((c) => (
            <option key={c.id} value={c.id}>
              {c.name}
            </option>
          ))}
        </select>
      </div>
      {entries.length === 0 ? (
        <p className="py-6 text-center text-sm text-muted-foreground">No calls match.</p>
      ) : (
        <ul
          aria-label="Calls"
          className="flex flex-col divide-y divide-border rounded-lg border border-border"
        >
          {entries.map((e) => (
            <div key={e.id}>
              <AuditRow entry={e} />
              <p className="px-3 pb-2 text-2xs text-muted-foreground">
                {names.get(e.composer_id ?? "") ?? "A Composer"}
              </p>
            </div>
          ))}
        </ul>
      )}
    </div>
  );
}
