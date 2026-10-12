import type { Composer } from "@circles/bindings/Composer";
import { Badge } from "@circles/ui/components/badge";
import { Button } from "@circles/ui/components/button";
import { Input } from "@circles/ui/components/input";
import { PlusIcon, SearchIcon } from "lucide-react";
import { useState } from "react";
import { useNavigate } from "react-router";
import { ComposerOrb } from "@/components/composer-orb";
import { usePendingCards } from "@/features/genui/api";
import { CardView } from "@/features/genui/card-view";
import { formatAge } from "@/lib/time";
import { useApprovals, useComposerRuns, useComposers, useComposersStatus } from "./api";
import { ApprovalCard } from "./approval-card";
import { SharedComposers } from "./shared";
import { composerState } from "./status";
import { useComposerDialog } from "./store";
import { TEMPLATES } from "./templates";

function StateBadge({
  label,
  tone,
}: {
  label: string;
  tone: "running" | "waiting" | "idle" | "new";
}) {
  const style =
    tone === "running"
      ? "bg-success/15 text-success"
      : tone === "waiting"
        ? "bg-warning/15 text-warning"
        : "";
  return (
    <Badge variant={style ? "outline" : "secondary"} className={`shrink-0 gap-1.5 ${style}`}>
      {tone === "running" || tone === "waiting" ? (
        <span
          aria-hidden
          className={`size-1.5 rounded-full ${tone === "running" ? "bg-success" : "bg-warning"}`}
        />
      ) : null}
      {label}
    </Badge>
  );
}

function ComposerCard({ composer }: { composer: Composer }) {
  const navigate = useNavigate();
  const runs = useComposerRuns(composer.id).data ?? [];
  const state = composerState(runs);
  const last = runs[0]?.session.updated_at;
  return (
    <li>
      <button
        type="button"
        onClick={() => navigate(`/composer/${composer.id}`)}
        className="flex h-full w-full flex-col gap-1.5 rounded-lg border border-border p-3 text-left outline-none hover:bg-muted/50 focus-visible:ring-2 focus-visible:ring-ring/50"
      >
        <span className="flex items-center gap-2 text-sm font-medium">
          <ComposerOrb seed={composer.id} className="size-5" />
          <span className="min-w-0 flex-1 truncate">{composer.name}</span>
          <StateBadge {...state} />
        </span>
        {composer.instructions ? (
          <span className="line-clamp-2 text-xs text-muted-foreground">
            {composer.instructions}
          </span>
        ) : null}
        {last ? (
          <span className="text-xs text-muted-foreground">Last run {formatAge(last)}</span>
        ) : null}
      </button>
    </li>
  );
}

/** Composers: manager agents that create workspaces, start agents in them and follow them. */
export function ComposersHome() {
  const composers = useComposers().data ?? [];
  const status = useComposersStatus().data;
  const pending = useApprovals({ onlyPending: true }).data ?? [];
  const openCreate = useComposerDialog((s) => s.openCreate);
  const names = new Map(composers.map((c) => [c.id, c.name]));
  const cards = usePendingCards().data ?? [];
  const [search, setSearch] = useState("");
  const query = search.trim().toLowerCase();
  const shown = composers.filter(
    (c) =>
      !query ||
      c.name.toLowerCase().includes(query) ||
      c.instructions.toLowerCase().includes(query),
  );

  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-6 px-6 py-8">
      <header className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <h1 className="text-xl font-semibold">Composers</h1>
          <p className="text-sm text-muted-foreground">
            A Composer manages work for you: it creates workspaces, starts agents in them and
            follows their progress.
          </p>
        </div>
        <Button size="sm" className="shrink-0" onClick={() => openCreate()}>
          <PlusIcon />
          New Composer
        </Button>
      </header>

      {status?.paused ? (
        <p role="status" className="rounded-md border border-warning/50 bg-warning/10 p-3 text-sm">
          Composers are paused. Every tool call is refused until you resume them. Resume them in
          Settings, Composers.
        </p>
      ) : null}

      {pending.length > 0 || cards.length > 0 ? (
        <section aria-label="Waiting for you" className="flex flex-col gap-2">
          <h2 className="text-sm font-medium">Waiting for you</h2>
          {cards.map((c) => (
            <CardView
              key={c.uid}
              stored={c}
              from={(c.composer_id && names.get(c.composer_id)) || "An agent"}
            />
          ))}
          {pending.map((a) => (
            <div key={a.id} className="flex flex-col gap-1">
              <p className="text-xs text-muted-foreground">
                {names.get(a.composer_id) ?? "A Composer"}
              </p>
              <ApprovalCard approval={a} />
            </div>
          ))}
        </section>
      ) : null}

      <section aria-label="Templates" className="flex flex-col gap-2">
        <h2 className="text-sm font-medium">Templates</h2>
        <ul className="grid grid-cols-1 gap-2 sm:grid-cols-2">
          {TEMPLATES.map((t) => (
            <li key={t.id}>
              <button
                type="button"
                onClick={() => openCreate(t.input)}
                className="flex h-full w-full flex-col gap-1 rounded-lg border border-dashed border-border p-3 text-left outline-none hover:bg-muted/50 focus-visible:ring-2 focus-visible:ring-ring/50"
              >
                <span className="text-sm font-medium">{t.name}</span>
                <span className="text-xs text-muted-foreground">{t.description}</span>
              </button>
            </li>
          ))}
        </ul>
      </section>

      <section aria-label="Your Composers" className="flex flex-col gap-2">
        <div className="flex items-center justify-between gap-3">
          <h2 className="text-sm font-medium">
            Your Composers
            {composers.length > 0 ? (
              <span className="ml-1.5 font-normal text-muted-foreground">{composers.length}</span>
            ) : null}
          </h2>
          {composers.length > 0 ? (
            <div className="relative w-56">
              <SearchIcon className="pointer-events-none absolute top-2 left-2.5 size-3.5 text-muted-foreground" />
              <Input
                type="search"
                aria-label="Search Composers"
                placeholder="Search Composers"
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                className="h-7 pl-8 text-xs"
              />
            </div>
          ) : null}
        </div>
        {composers.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            You have none yet. Start from a template above or make your own.
          </p>
        ) : shown.length === 0 ? (
          <p className="text-sm text-muted-foreground">No Composer matches “{search.trim()}”.</p>
        ) : (
          <ul className="grid grid-cols-1 gap-2 sm:grid-cols-2">
            {shown.map((c) => (
              <ComposerCard key={c.id} composer={c} />
            ))}
          </ul>
        )}
      </section>

      <SharedComposers />
    </div>
  );
}
