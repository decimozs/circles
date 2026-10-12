import { useEffect, useRef } from "react";
import { type PanelSize, usePanelRef } from "react-resizable-panels";

/**
 * Keeps a collapsible panel in sync with the store.
 * Dragging a panel to its collapse point writes back to the store, and
 * programmatic collapses (zen, route changes) never do.
 */
export function usePanelSync(options: {
  visible: boolean;
  canShow: boolean;
  setOpen: (open: boolean) => void;
  setWidth: (width: number) => void;
  /** Size the panel keeps when collapsed. Zero hides it completely. */
  collapsedPx?: number;
  /** Whether a size should be remembered. Sizes the panel takes by itself should not. */
  remember?: () => boolean;
}) {
  const ref = usePanelRef();
  const { visible, canShow, setOpen, setWidth, collapsedPx = 0, remember = () => true } = options;
  const visibleRef = useRef(visible);
  visibleRef.current = visible;

  useEffect(() => {
    const panel = ref.current;
    if (!panel) return;
    if (visible && panel.isCollapsed()) panel.expand();
    if (!visible && !panel.isCollapsed()) panel.collapse();
  }, [visible, ref]);

  const onResize = (size: PanelSize) => {
    if (size.inPixels <= collapsedPx) {
      if (visibleRef.current) setOpen(false);
      return;
    }
    if (!visibleRef.current && canShow) setOpen(true);
    if (remember()) setWidth(Math.round(size.inPixels));
  };

  return { ref, onResize };
}
