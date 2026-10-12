import { type KeyboardEvent, useCallback, useEffect, useRef } from "react";

const RESIZE_KEYS = new Set(["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End"]);

/**
 * Tells whether the user is the one resizing a panel, by a drag or by the keys on a focused
 * divider. A panel also changes size by itself, when the window shrinks or while it first lays
 * out, and those sizes are not choices to remember.
 */
export function useUserResizing() {
  const active = useRef(false);
  const timer = useRef<number | undefined>(undefined);

  const stopSoon = useCallback((after: number) => {
    window.clearTimeout(timer.current);
    // The last size arrives just after the pointer is released.
    timer.current = window.setTimeout(() => {
      active.current = false;
    }, after);
  }, []);

  useEffect(() => {
    const release = () => stopSoon(150);
    window.addEventListener("pointerup", release);
    window.addEventListener("pointercancel", release);
    return () => {
      window.removeEventListener("pointerup", release);
      window.removeEventListener("pointercancel", release);
      window.clearTimeout(timer.current);
    };
  }, [stopSoon]);

  const handlers = {
    onPointerDownCapture: () => {
      window.clearTimeout(timer.current);
      active.current = true;
    },
    onKeyDownCapture: (event: KeyboardEvent) => {
      if (!RESIZE_KEYS.has(event.key)) return;
      active.current = true;
      stopSoon(300);
    },
  };

  return { isResizing: () => active.current, handlers };
}
