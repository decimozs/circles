import { useEffect } from "react";
import { flattenActions, useActionRegistry } from "@/lib/actions";
import { hasModifier, isTypingTarget, matchesShortcut } from "@/lib/shortcuts";

/** Runs the registered action whose shortcut matches the key event. Mount once. */
export function useGlobalShortcuts() {
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.isComposing) return;
      const typing = isTypingTarget(e.target);
      const actions = flattenActions(useActionRegistry.getState().sources);
      for (const action of actions) {
        for (const shortcut of action.shortcuts ?? []) {
          if (typing && !hasModifier(shortcut)) continue;
          if (!matchesShortcut(e, shortcut)) continue;
          e.preventDefault();
          action.run();
          return;
        }
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
}
