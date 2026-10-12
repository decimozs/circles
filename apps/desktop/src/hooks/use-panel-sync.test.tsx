import { act, fireEvent, render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { usePanelSync } from "./use-panel-sync";
import { useUserResizing } from "./use-user-resizing";

type Sync = ReturnType<typeof usePanelSync>;
const size = (inPixels: number) => ({ asPercentage: 0, inPixels });

function Probe({
  setWidth,
  onReady,
}: {
  setWidth: (w: number) => void;
  onReady: (s: Sync, r: ReturnType<typeof useUserResizing>) => void;
}) {
  const resizing = useUserResizing();
  const sync = usePanelSync({
    visible: true,
    canShow: true,
    setOpen: () => {},
    setWidth,
    remember: resizing.isResizing,
  });
  onReady(sync, resizing);
  return <div data-testid="handle" {...resizing.handlers} />;
}

function setup() {
  const setWidth = vi.fn();
  let sync!: Sync;
  const view = render(
    <Probe
      setWidth={setWidth}
      onReady={(s) => {
        sync = s;
      }}
    />,
  );
  return { setWidth, sync: () => sync, handle: view.getByTestId("handle") };
}

describe("remembering panel sizes", () => {
  it("ignores a size the panel took by itself, such as the smallest in a narrow window", () => {
    const { setWidth, sync } = setup();
    sync().onResize(size(220));
    expect(setWidth).not.toHaveBeenCalled();
  });

  it("remembers a size set by dragging, until the pointer is released", async () => {
    vi.useFakeTimers();
    const { setWidth, sync, handle } = setup();
    fireEvent.pointerDown(handle);
    sync().onResize(size(412.4));
    expect(setWidth).toHaveBeenLastCalledWith(412);
    fireEvent.pointerUp(window);
    sync().onResize(size(430)); // still part of the drag, just after release
    expect(setWidth).toHaveBeenLastCalledWith(430);
    await act(async () => {
      vi.advanceTimersByTime(300);
    });
    sync().onResize(size(250)); // the window shrank afterwards
    expect(setWidth).toHaveBeenCalledTimes(2);
    vi.useRealTimers();
  });

  it("remembers a size set with the arrow keys on the divider", async () => {
    vi.useFakeTimers();
    const { setWidth, sync, handle } = setup();
    fireEvent.keyDown(handle, { key: "Tab" });
    sync().onResize(size(300));
    expect(setWidth).not.toHaveBeenCalled();
    fireEvent.keyDown(handle, { key: "ArrowRight" });
    sync().onResize(size(310));
    expect(setWidth).toHaveBeenLastCalledWith(310);
    await act(async () => {
      vi.advanceTimersByTime(400);
    });
    sync().onResize(size(200));
    expect(setWidth).toHaveBeenCalledTimes(1);
    vi.useRealTimers();
  });
});
