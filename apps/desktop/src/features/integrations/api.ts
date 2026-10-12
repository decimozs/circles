import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { ipc } from "@/lib/ipc";
import { SETTINGS_PAGES } from "@/lib/settings-pages";

export const integrationKeys = { jira: ["jira-status"] as const };

/** Keeps the connection state fresh when it changes, from here or from another window. */
function useIntegrationSync() {
  const queryClient = useQueryClient();
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    listen("integrations:changed", () => {
      queryClient.invalidateQueries({ queryKey: integrationKeys.jira });
      queryClient.invalidateQueries({ queryKey: ["composers"] });
    })
      .then((fn) => {
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

/** The settings pages that apply now: a service has its own page only while it is connected. */
export function useSettingsPages() {
  const jira = useJiraStatus().data;
  return SETTINGS_PAGES.filter((p) => p.id !== "jira" || jira?.connected);
}

export function useJiraStatus() {
  useIntegrationSync();
  return useQuery({ queryKey: integrationKeys.jira, queryFn: ipc.jiraStatus });
}

export function useConnectJira() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (v: { site: string; email: string; token: string }) =>
      ipc.jiraConnect(v.site, v.email, v.token),
    onSuccess: (status) => queryClient.setQueryData(integrationKeys.jira, status),
  });
}

export function useSignInJira() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ipc.jiraSignIn,
    onSuccess: (status) => queryClient.setQueryData(integrationKeys.jira, status),
  });
}

export function useChooseJiraSite() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (cloudId: string) => ipc.jiraChooseSite(cloudId),
    onSuccess: (status) => queryClient.setQueryData(integrationKeys.jira, status),
  });
}

export function useDisconnectJira() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ipc.jiraDisconnect,
    onSuccess: (status) => {
      queryClient.setQueryData(integrationKeys.jira, status);
      queryClient.invalidateQueries({ queryKey: ["composers"] });
    },
  });
}
