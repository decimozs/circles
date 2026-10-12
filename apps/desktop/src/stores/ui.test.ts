import { describe, expect, it } from "vitest";
import { DOCK, RIGHT_PANEL, SIDEBAR, useUi } from "./ui";

type Migrate = (persisted: unknown, version: number) => unknown;
const migrate = (persisted: Record<string, unknown>, version = 4) =>
  (useUi.persist.getOptions().migrate as Migrate)(persisted, version) as Record<string, number>;

describe("panel widths", () => {
  it("start a little wider than they used to", () => {
    expect(SIDEBAR.default).toBe(320);
    expect(RIGHT_PANEL.default).toBe(490);
    expect(useUi.getInitialState().leftWidth).toBe(SIDEBAR.default);
    expect(useUi.getInitialState().rightWidth).toBe(RIGHT_PANEL.default);
  });

  it("leave room for the center column in a window of the default size", () => {
    // The window opens 1280 wide, and the center needs at least 320. In a smaller window the
    // panels give way.
    expect(SIDEBAR.default + RIGHT_PANEL.default + 320).toBeLessThanOrEqual(1280);
  });

  it("start again from the defaults for everyone once, whatever was saved", () => {
    // The widths saved so far include sizes the panels took by themselves, such as the smallest.
    for (const [left, right] of [
      [300, 380],
      [220, 280],
      [360, 500],
    ]) {
      const moved = migrate({ leftWidth: left, rightWidth: right }, 4);
      expect(moved.leftWidth).toBe(SIDEBAR.default);
      expect(moved.rightWidth).toBe(RIGHT_PANEL.default);
    }
    expect(migrate({ leftWidth: 220 }, 3).leftWidth).toBe(SIDEBAR.default);
  });

  it("move the widths of the previous default, and keep ones the user set", () => {
    const untouched = migrate({ leftWidth: 340, rightWidth: 480 }, 5);
    expect(untouched.leftWidth).toBe(SIDEBAR.default);
    expect(untouched.rightWidth).toBe(RIGHT_PANEL.default);
    const chosen = migrate({ leftWidth: 360, rightWidth: 520 }, 5);
    expect(chosen.leftWidth).toBe(360);
    expect(chosen.rightWidth).toBe(520);
  });

  it("move the right pane off the previous default only", () => {
    expect(migrate({ rightWidth: 430, leftWidth: 320 }, 6).rightWidth).toBe(RIGHT_PANEL.default);
    expect(migrate({ rightWidth: 450 }, 6).rightWidth).toBe(450);
    expect(migrate({ rightWidth: 430, leftWidth: 320 }, 6).leftWidth).toBe(320);
  });

  it("move the right pane off the default of version 7 only", () => {
    expect(migrate({ rightWidth: 460 }, 7).rightWidth).toBe(RIGHT_PANEL.default);
    expect(migrate({ rightWidth: 470 }, 7).rightWidth).toBe(470);
  });

  it("keep the other saved settings when it does", () => {
    const moved = migrate({ leftOpen: false, dockHeight: 333, leftWidth: 220 }, 4) as Record<
      string,
      unknown
    >;
    expect(moved.leftOpen).toBe(false);
    expect(moved.dockHeight).toBe(333);
  });
});

describe("terminal panel", () => {
  it("starts closed and hides completely rather than leaving a bar behind", () => {
    expect(useUi.getInitialState().dockOpen).toBe(false);
    expect(DOCK.collapsed).toBe(0);
  });

  it("opens at a share of the window until the user drags it", () => {
    expect(DOCK.share).toBe(0.3);
    expect(useUi.getInitialState().dockHeight).toBeNull();
  });

  it("forgets a height that was only an old default, and keeps one the user chose", () => {
    expect(migrate({ dockHeight: 280 }, 9).dockHeight).toBeNull();
    expect(migrate({ dockHeight: 360 }, 10).dockHeight).toBeNull();
    expect(migrate({ dockHeight: 300 }, 10).dockHeight).toBe(300);
  });

  it("is closed once for everyone when it moved to the bottom of the chat", () => {
    expect(migrate({ dockOpen: true, dockHeight: 300 }, 8).dockOpen).toBe(false);
    expect(migrate({ dockOpen: true, dockHeight: 300 }, 8).dockHeight).toBe(300);
  });
});
