import type { Composer } from "@circles/bindings/Composer";
import { Button } from "@circles/ui/components/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@circles/ui/components/dropdown-menu";
import { cn } from "@circles/ui/utils";
import { PinIcon, PlusIcon } from "lucide-react";
import { NavLink, useMatch, useNavigate } from "react-router";
import { ComposerOrb } from "@/components/composer-orb";
import { useComposerRuns, useComposers, usePinComposer } from "./api";
import { composerState } from "./status";
import { useComposerDialog } from "./store";

function ComposerMenu({ composer }: { composer: Composer }) {
  const pin = usePinComposer();
  const navigate = useNavigate();
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={`Actions for ${composer.name}`}
            className="size-6 text-muted-foreground opacity-0 group-hover/composer:opacity-100 focus-visible:opacity-100 data-popup-open:opacity-100"
          />
        }
      >
        <span aria-hidden>···</span>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-44">
        <DropdownMenuItem onClick={() => navigate(`/composer/${composer.id}`)}>
          Open
        </DropdownMenuItem>
        <DropdownMenuItem onClick={() => pin.mutate({ id: composer.id, pinned: !composer.pinned })}>
          {composer.pinned ? "Unpin" : "Pin"}
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem onClick={() => navigate("/composers/audit")}>Audit log</DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/**
 * The ring around a Composer's circle shows only while it has a status. It is a solid line that
 * moves: an arc that turns while it works, and a steady ring that pulses while it waits for you.
 * Otherwise it is clear, so the circle does not move when it comes and goes.
 */
const RING: Record<string, string> = {
  running: "border-success border-t-transparent animate-spin",
  waiting: "border-warning animate-pulse",
  idle: "border-transparent",
  new: "border-transparent",
};

/**
 * One Composer in the sidebar: its name, and nothing under it. A Composer has one chat, so there is
 * no list of runs. What it is doing shows as a moving ring around its circle, and nowhere else.
 */
function ComposerGroup({ composer }: { composer: Composer }) {
  const runs = useComposerRuns(composer.id).data ?? [];
  const state = composerState(runs);
  const active = useMatch(`/composer/${composer.id}/*`) !== null;
  return (
    <div
      data-tone={state.tone}
      className={cn(
        "group/composer flex h-9 in-data-[density=compact]:h-7 items-center gap-1 rounded-md pr-1 transition-colors hover:bg-sidebar-accent",
        // The same highlight as any other item of the sidebar. No border, and no colour for status.
        active && "bg-sidebar-accent",
      )}
    >
      <NavLink
        to={`/composer/${composer.id}`}
        title={state.label}
        className="flex h-full min-w-0 flex-1 items-center gap-2 rounded-md pl-2 text-left text-sm font-medium outline-none focus-visible:ring-2 focus-visible:ring-sidebar-ring/60"
      >
        {/* The status is a ring right against the circle, with no gap, in the colour of what it is doing. */}
        <span className="relative flex shrink-0 rounded-full p-px">
          <span
            aria-hidden
            data-testid="composer-ring"
            className={cn("absolute inset-0 rounded-full border", RING[state.tone])}
          />
          <ComposerOrb seed={composer.id} className="size-4" />
        </span>
        <span className="truncate">{composer.name}</span>
        <span className="sr-only">{state.label}</span>
      </NavLink>
      <ComposerMenu composer={composer} />
    </div>
  );
}

/** Pinned Composers, for the Pinned section: a link each, like a pinned workspace. */
export function PinnedComposers() {
  const pinned = (useComposers().data ?? []).filter((c) => c.pinned);
  return (
    <>
      {pinned.map((c) => (
        <NavLink
          key={c.id}
          to={`/composer/${c.id}`}
          className={({ isActive }) =>
            cn(
              "flex h-8 in-data-[density=compact]:h-6 items-center gap-2 rounded-md px-2 text-sm outline-none transition-colors hover:bg-sidebar-accent",
              isActive
                ? "bg-sidebar-accent text-sidebar-accent-foreground"
                : "text-sidebar-foreground",
            )
          }
        >
          <ComposerOrb seed={c.id} className="size-3.5" />
          <span className="truncate">{c.name}</span>
          <PinIcon className="ml-auto size-3 text-muted-foreground" />
        </NavLink>
      ))}
    </>
  );
}

export function hasPinnedComposers(composers: Composer[]): boolean {
  return composers.some((c) => c.pinned);
}

/** The Composers group of the sidebar. Not shown until there is a Composer. */
export function ComposersSection() {
  const composers = (useComposers().data ?? []).filter((c) => !c.pinned);
  const openCreate = useComposerDialog((s) => s.openCreate);
  if (composers.length === 0) return null;
  return (
    <section aria-label="Composers" className="flex flex-col gap-0.5">
      <h2 className="flex items-center justify-between px-2 py-1 text-xs text-muted-foreground">
        Composers
        <button
          type="button"
          aria-label="New Composer"
          onClick={() => openCreate()}
          className="rounded-sm p-0.5 hover:bg-sidebar-accent hover:text-foreground"
        >
          <PlusIcon className="size-3" />
        </button>
      </h2>
      {composers.map((c) => (
        <ComposerGroup key={c.id} composer={c} />
      ))}
    </section>
  );
}
