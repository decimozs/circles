import { keepPreviousData, useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { ipc } from "@/lib/ipc";
import { type RangeId, sinceFor } from "./format";

export const usageKeys = {
  summary: (range: RangeId, folder: string | null) => ["usage", range, folder] as const,
  all: ["usage"] as const,
  limit: ["usage-limit"] as const,
};

/** Usage over a range, for one Claude folder or all of them. This reads what Circles kept, which
 * takes milliseconds, so it is never slow; a refresh in the background keeps it up to date and
 * `useUsageSync` tells this query when it changed. While another range or folder loads, the
 * numbers on screen stay. */
export function useUsage(range: RangeId, folder: string | null, enabled: boolean) {
  return useQuery({
    queryKey: usageKeys.summary(range, folder),
    queryFn: () => ipc.usageSummary(sinceFor(range), folder),
    enabled,
    staleTime: Number.POSITIVE_INFINITY,
    placeholderData: keepPreviousData,
  });
}

/** Reads again when the background refresh adds something. */
export function useUsageSync() {
  const queryClient = useQueryClient();
  useEffect(() => {
    let cancelled = false;
    let stop: (() => void) | undefined;
    listen("usage:changed", () => queryClient.invalidateQueries({ queryKey: usageKeys.all }))
      .then((unlisten) => {
        if (cancelled) unlisten();
        else stop = unlisten;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      stop?.();
    };
  }, [queryClient]);
}

export function useRateLimit(enabled: boolean) {
  return useQuery({ queryKey: usageKeys.limit, queryFn: ipc.claudeRateLimit, enabled });
}
