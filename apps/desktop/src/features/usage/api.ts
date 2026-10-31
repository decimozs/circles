import { useQuery } from "@tanstack/react-query";
import { ipc } from "@/lib/ipc";
import { type RangeId, sinceFor } from "./format";

export const usageKeys = {
  summary: (range: RangeId, folder: string | null) => ["usage", range, folder] as const,
  limit: ["usage-limit"] as const,
};

/** Usage over a range, for one Claude folder or all of them. Reading the history takes a few
 * seconds the first time, so the answer is kept for a while. */
export function useUsage(range: RangeId, folder: string | null, enabled: boolean) {
  return useQuery({
    queryKey: usageKeys.summary(range, folder),
    queryFn: () => ipc.usageSummary(sinceFor(range), folder),
    enabled,
    staleTime: 30_000,
  });
}

export function useRateLimit(enabled: boolean) {
  return useQuery({ queryKey: usageKeys.limit, queryFn: ipc.claudeRateLimit, enabled });
}
