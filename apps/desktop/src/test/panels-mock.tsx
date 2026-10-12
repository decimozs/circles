/**
 * Stand-in for `react-resizable-panels` in jsdom. jsdom has no layout, so the real library sees
 * every pointer event as a hit on a divider and steals focus from inputs.
 */
import { type ReactNode, type Ref, useEffect, useRef } from "react";

export function Group({ children }: { children?: ReactNode }) {
  return <div>{children}</div>;
}

type PanelHandle = {
  collapse: () => void;
  expand: () => void;
  isCollapsed: () => boolean;
  getSize: () => { asPercentage: number; inPixels: number };
  resize: (size: number | string) => void;
};

export function Panel({
  children,
  panelRef,
  id,
}: {
  children?: ReactNode;
  panelRef?: Ref<PanelHandle | null>;
  id?: string;
}) {
  const element = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (panelRef && typeof panelRef === "object") {
      // A collapse is kept on the element, so a test can see which panels are collapsed.
      const set = (collapsed: boolean) => {
        if (element.current) element.current.dataset.collapsed = String(collapsed);
      };
      (panelRef as { current: PanelHandle | null }).current = {
        collapse: () => set(true),
        expand: () => set(false),
        isCollapsed: () => element.current?.dataset.collapsed === "true",
        getSize: () => ({
          asPercentage: 0,
          inPixels: Number.parseFloat(element.current?.dataset.size ?? "0") || 0,
        }),
        resize: (size: number | string) => {
          if (element.current) element.current.dataset.size = String(size);
        },
      };
    }
  }, [panelRef]);
  return (
    <div ref={element} data-panel-id={id}>
      {children}
    </div>
  );
}

export function Separator() {
  return <hr />;
}

export function usePanelRef() {
  return useRef<PanelHandle | null>(null);
}
