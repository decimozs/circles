import type { Autonomy } from "@circles/bindings/Autonomy";
import type { Composer } from "@circles/bindings/Composer";
import type { ComposerInput } from "@circles/bindings/ComposerInput";
import type { Tier } from "@circles/bindings/Tier";
import type { TriggerInput } from "@circles/bindings/TriggerInput";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { ipc } from "@/lib/ipc";

export const composerKeys = {
  all: ["composers"] as const,
  runs: (composerId: string) => ["composer-runs", composerId] as const,
  approvals: (composerId?: string, sessionId?: string, onlyPending?: boolean) =>
    ["composer-approvals", composerId ?? null, sessionId ?? null, onlyPending ?? false] as const,
  fleet: (composerId: string, sessionId?: string) =>
    ["composer-fleet", composerId, sessionId ?? null] as const,
  audit: (composerId?: string, sessionId?: string, search?: string) =>
    ["composer-audit", composerId ?? null, sessionId ?? null, search ?? ""] as const,
  status: ["composers-status"] as const,
  triggers: (composerId?: string) => ["composer-triggers", composerId ?? null] as const,
  tools: ["tool-catalog"] as const,
};

/** How often what is on screen is looked at again, in case an event was missed. */
const WATCH_MS = 4000;

export function useComposers() {
  return useQuery({ queryKey: composerKeys.all, queryFn: ipc.composerList });
}

export function useComposer(id: string | undefined) {
  const list = useComposers();
  return { ...list, data: list.data?.find((c) => c.id === id) };
}

export function useComposerRuns(composerId: string | undefined, includeClosed = false) {
  return useQuery({
    queryKey: [...composerKeys.runs(composerId ?? ""), includeClosed],
    queryFn: () => ipc.composerRuns(composerId as string, includeClosed),
    enabled: !!composerId,
  });
}

export function useApprovals(filter: {
  composerId?: string;
  sessionId?: string;
  onlyPending?: boolean;
}) {
  return useQuery({
    queryKey: composerKeys.approvals(filter.composerId, filter.sessionId, filter.onlyPending),
    queryFn: () => ipc.composerApprovals(filter),
    refetchInterval: WATCH_MS,
  });
}

export function useFleet(composerId: string | undefined, sessionId?: string) {
  return useQuery({
    queryKey: composerKeys.fleet(composerId ?? "", sessionId),
    queryFn: () => ipc.composerFleet(composerId as string, sessionId),
    enabled: !!composerId,
    refetchInterval: WATCH_MS,
  });
}

export function useAudit(filter: { composerId?: string; sessionId?: string; search?: string }) {
  return useQuery({
    queryKey: composerKeys.audit(filter.composerId, filter.sessionId, filter.search),
    queryFn: () => ipc.auditList({ ...filter, limit: 300 }),
    refetchInterval: WATCH_MS,
  });
}

export function useComposersStatus() {
  return useQuery({
    queryKey: composerKeys.status,
    queryFn: ipc.composersStatus,
    refetchInterval: WATCH_MS,
  });
}

/** Which workspaces a Composer started, by workspace id. */
export function useWorkspaceLinks() {
  const links = useQuery({
    queryKey: ["composer-workspace-links"],
    queryFn: ipc.composerWorkspaceLinks,
    refetchInterval: WATCH_MS * 3,
  });
  return new Map((links.data ?? []).map((l) => [l.workspace_id, l]));
}

export function useToolCatalog() {
  return useQuery({ queryKey: composerKeys.tools, queryFn: ipc.toolCatalog, staleTime: Infinity });
}

export function useTriggers(composerId?: string) {
  return useQuery({
    queryKey: composerKeys.triggers(composerId),
    queryFn: () => ipc.triggerList(composerId),
  });
}

function useRefreshing<TVars, TData>(
  fn: (vars: TVars) => Promise<TData>,
  keys: readonly unknown[][],
) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: fn,
    onSettled: () => {
      for (const key of keys) queryClient.invalidateQueries({ queryKey: key });
    },
  });
}

export const useCreateComposer = () =>
  useRefreshing((input: ComposerInput) => ipc.composerCreate(input), [[...composerKeys.all]]);
export const useUpdateComposer = () =>
  useRefreshing(
    (vars: { id: string; input: ComposerInput }) => ipc.composerUpdate(vars.id, vars.input),
    [[...composerKeys.all]],
  );
export const usePinComposer = () =>
  useRefreshing(
    (vars: { id: string; pinned: boolean }) => ipc.composerPin(vars.id, vars.pinned),
    [[...composerKeys.all]],
  );
export const useSetMode = () =>
  useRefreshing(
    (vars: { id: string; mode: string }) => ipc.composerSetMode(vars.id, vars.mode),
    [[...composerKeys.all]],
  );
export const useSetNotes = () =>
  useRefreshing(
    (vars: { id: string; notes: string }) => ipc.composerSetNotes(vars.id, vars.notes),
    [[...composerKeys.all]],
  );
export const useDeleteComposer = () =>
  useRefreshing(
    (id: string) => ipc.composerDelete(id),
    [[...composerKeys.all], ["composer-runs"], ["composer-triggers"]],
  );
export const useStartRun = () =>
  useRefreshing(
    (vars: { composerId: string; message?: string }) =>
      ipc.composerRunStart(vars.composerId, vars.message),
    [["composer-runs"], ["sessions"]],
  );
export const useAnswerApproval = () =>
  useRefreshing(
    (vars: { id: string; allow: boolean; forRun?: boolean }) =>
      ipc.approvalAnswer(vars.id, vars.allow, vars.forRun),
    [["composer-approvals"], ["composers-status"], ["composer-runs"], ["composer-audit"]],
  );
export const useStopChild = () =>
  useRefreshing((chatId: string) => ipc.composerStopChild(chatId), [["composer-fleet"]]);
export const useSaveTrigger = () =>
  useRefreshing(
    (vars: { composerId: string; id?: string; input: TriggerInput }) =>
      vars.id
        ? ipc.triggerUpdate(vars.id, vars.input)
        : ipc.triggerCreate(vars.composerId, vars.input),
    [["composer-triggers"]],
  );
export const useDeleteTrigger = () =>
  useRefreshing((id: string) => ipc.triggerDelete(id), [["composer-triggers"]]);
export const usePause = () =>
  useRefreshing(
    (paused: boolean) => ipc.composersPause(paused),
    [[...composerKeys.status], ["composer-approvals"]],
  );
export const useStopAll = () =>
  useRefreshing(
    () => ipc.composersStopAll(),
    [
      [...composerKeys.status],
      ["composer-approvals"],
      ["composer-runs"],
      ["composer-fleet"],
      ["sessions"],
    ],
  );

/** Refreshes approvals and runs when the backend says something changed. Mount once. */
export function useComposerSync() {
  const queryClient = useQueryClient();
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    const refresh = () => {
      for (const key of [
        ["composer-approvals"],
        ["composers-status"],
        ["composer-runs"],
        ["composer-audit"],
      ]) {
        queryClient.invalidateQueries({ queryKey: key });
      }
    };
    // A run's status lives in its chat, so a chat finishing refreshes the runs too.
    Promise.all([listen("composer:approvals-changed", refresh), listen("session:changed", refresh)])
      .then((fns) => {
        const fn = () => {
          for (const f of fns) f();
        };
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {
        // Not running inside Tauri: nothing to listen to.
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [queryClient]);
}

// What a tool does when nothing is set.

export function defaultAutonomy(tier: Tier): Autonomy {
  return tier === "read" ? "allow" : "ask";
}

/** The same four choices a chat has for its permission mode. */
export const MODES = [
  { value: "manual", label: "Ask", hint: "Asks before it changes anything." },
  {
    value: "accept_edits",
    label: "Accept edits",
    hint: "Creates workspaces and starts agents on its own. Asks before opening or merging pull requests, or deleting.",
  },
  { value: "plan", label: "Plan", hint: "Only looks and makes a plan. It cannot change anything." },
  {
    value: "bypass",
    label: "Full access",
    hint: "Does everything on its own, including opening and merging pull requests and deleting.",
  },
] as const;

/** What a mode does with a tool of this tier. Mirrors `Mode::autonomy` in the backend. */
export function modeAutonomy(mode: string, tier: Tier): Autonomy {
  if (mode === "bypass") return "allow";
  if (tier === "read") return "allow";
  if (mode === "accept_edits") return tier === "write" ? "allow" : "ask";
  if (mode === "plan") return "off";
  return "ask";
}

/** What the Composer does with a tool: its own setting, else what its mode says. */
export function autonomyOf(
  composer: Pick<Composer, "autonomy" | "mode">,
  tool: string,
  tier: Tier,
): Autonomy {
  return (
    (composer.autonomy as Record<string, Autonomy | undefined>)[tool] ??
    modeAutonomy(composer.mode, tier)
  );
}

export const TIER_LABEL: Record<Tier, string> = {
  read: "Look",
  write: "Change on this Mac",
  external: "Visible outside this Mac",
  destructive: "Delete or remove",
};

/** A tool that can do harm outside the Mac or delete, which deserves a warning when allowed. */
export const isRisky = (tier: Tier) => tier === "external" || tier === "destructive";

/**
 * An example first message for a Composer, from its name and instructions. `null` while it is
 * being made, when the setting is off, or when there is no model. Made once per version of the
 * Composer.
 */
export function useFirstPrompt(
  composer: { id: string; name: string; instructions: string } | undefined,
) {
  return useQuery({
    queryKey: ["first-prompt", composer?.id, composer?.name, composer?.instructions],
    queryFn: () => ipc.smartFirstPrompt(composer?.name ?? "", composer?.instructions ?? ""),
    enabled: !!composer,
    // A missing answer (the model was not ready) is tried again after a few minutes.
    staleTime: 5 * 60_000,
    retry: false,
  }).data;
}

/** The user's skills and commands that instructions can name with `/`. */
export function useComposerSkills() {
  return useQuery({ queryKey: ["composer-skills"], queryFn: () => ipc.composerSkills() });
}
