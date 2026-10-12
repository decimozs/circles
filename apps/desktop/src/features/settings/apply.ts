import { useEffect, useRef } from "react";
import { useTheme } from "@/components/theme-provider";
import { EFFORTS, MODELS } from "@/lib/models";
import { useChat } from "@/stores/chat";
import { useCreatePrefs } from "@/stores/create-prefs";
import { useSetSetting, useSettingsSync, useSettingsView } from "./api";

const FONT_SIZE: Record<string, string> = { small: "14px", medium: "", large: "18px" };

/**
 * Makes the app behave the way the settings file says. The file is the source of truth: a change
 * in it, made here or in an editor, shows up everywhere without a restart. Mount once.
 */
export function useApplySettings() {
  useSettingsSync();
  const settings = useSettingsView().data?.settings;
  const write = useSetSetting();
  const { theme, setTheme } = useTheme();

  const general = settings?.general;
  const appearance = settings?.appearance;
  const models = settings?.models;

  useEffect(() => {
    useChat.getState().setSendKey(general?.send_key === "mod_enter" ? "mod-enter" : "enter");
  }, [general?.send_key]);

  useEffect(() => {
    useChat.getState().setFollowUp(general?.follow_up === "queue" ? "queue" : "steer");
  }, [general?.follow_up]);

  // The file wins when it names a theme. The toggle in the sidebar writes its choice back.
  const fileTheme = appearance?.theme ?? null;
  useEffect(() => {
    if (fileTheme === "light" || fileTheme === "dark" || fileTheme === "system") {
      setTheme(fileTheme);
    }
  }, [fileTheme, setTheme]);
  const loaded = settings !== undefined;
  const mutate = write.mutate;
  const previous = useRef(theme);
  useEffect(() => {
    const before = previous.current;
    previous.current = theme;
    if (!loaded || before === theme || theme === (fileTheme ?? "system")) return;
    mutate({ scope: "user", path: ["appearance", "theme"], value: theme });
  }, [theme, loaded, fileTheme, mutate]);

  const fontSize = appearance?.font_size ?? "medium";
  useEffect(() => {
    document.documentElement.style.fontSize = FONT_SIZE[fontSize] ?? "";
  }, [fontSize]);

  const density = appearance?.density ?? "comfortable";
  useEffect(() => {
    document.documentElement.dataset.density = density;
  }, [density]);

  // The default from the settings is applied when it changes. After that, a pick made in the
  // Create dialog is kept, so the setting does not undo it at every launch.
  const model = models?.model ?? null;
  const effort = models?.effort ?? null;
  useEffect(() => {
    if (!loaded) return;
    const prefs = useCreatePrefs.getState();
    const patch: Parameters<typeof prefs.set>[0] = {};
    if (model !== prefs.appliedDefault.model) {
      const m = MODELS.find((x) => x.id === model);
      if (m) patch.model = m.id;
    }
    if (effort !== prefs.appliedDefault.effort) {
      const e = EFFORTS.find((x) => x.id === effort);
      if (e) patch.effort = e.id;
    }
    if (model !== prefs.appliedDefault.model || effort !== prefs.appliedDefault.effort) {
      patch.appliedDefault = { model, effort };
      prefs.set(patch);
    }
  }, [loaded, model, effort]);
}
