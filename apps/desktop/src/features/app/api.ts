import { useQuery } from "@tanstack/react-query";
import { ipc } from "@/lib/ipc";

export function useAppInfo() {
  return useQuery({
    queryKey: ["app-info"],
    queryFn: ipc.getAppInfo,
    staleTime: Number.POSITIVE_INFINITY,
  });
}

/** Name of this Mac, with a fallback while it loads or outside Tauri. */
export function useMachineName(): string {
  return useAppInfo().data?.machine_name ?? "Circles";
}
