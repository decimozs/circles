import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { ipc } from "@/lib/ipc";

export const skillKeys = {
  all: ["skills"] as const,
  detail: (id: string) => ["skills", "detail", id] as const,
  file: (id: string, path: string) => ["skills", "file", id, path] as const,
};

/** Every skill on this Mac and in the projects. Reading them is quick, so it is read again when
 * the window is focused, which is when a skill added in an editor is wanted. */
export function useSkills() {
  return useQuery({
    queryKey: skillKeys.all,
    queryFn: ipc.skillsList,
    staleTime: 10_000,
    refetchOnWindowFocus: true,
    placeholderData: keepPreviousData,
  });
}

/** One skill's text, front matter and files. */
export function useSkillDetail(id: string) {
  return useQuery({
    queryKey: skillKeys.detail(id),
    queryFn: () => ipc.skillsDetail(id),
    staleTime: 10_000,
  });
}

/** A file of a skill. `path` is empty until one is chosen. */
export function useSkillFile(id: string, path: string | null) {
  return useQuery({
    queryKey: skillKeys.file(id, path ?? ""),
    queryFn: () => ipc.skillsFile(id, path ?? ""),
    enabled: path !== null,
    staleTime: 10_000,
    retry: false,
  });
}
