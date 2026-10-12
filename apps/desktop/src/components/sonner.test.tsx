import { act, render, screen, waitFor } from "@testing-library/react";
import { toast } from "sonner";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Toaster } from "@/components/sonner";
import { ThemeProvider } from "@/components/theme-provider";
import { TOAST_MS, TOAST_WITH_ACTION_MS } from "@/lib/toast";

function show() {
  render(
    <ThemeProvider>
      <Toaster />
    </ThemeProvider>,
  );
}

describe("toasts", () => {
  afterEach(() => {
    toast.dismiss();
    vi.useRealTimers();
  });

  it("close on their own after a few seconds", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    show();
    act(() => void toast("Saved"));
    expect(await screen.findByText("Saved")).toBeInTheDocument();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(TOAST_MS - 500);
    });
    expect(screen.getByText("Saved")).toBeInTheDocument();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1500);
    });
    await waitFor(() => expect(screen.queryByText("Saved")).not.toBeInTheDocument());
  });

  it("with a button stay longer than a plain one, but still close", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    show();
    act(
      () =>
        void toast("Went back", {
          duration: TOAST_WITH_ACTION_MS,
          action: { label: "Undo", onClick: () => {} },
        }),
    );
    expect(await screen.findByText("Went back")).toBeInTheDocument();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(TOAST_MS + 1000);
    });
    expect(screen.getByText("Went back")).toBeInTheDocument();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(TOAST_WITH_ACTION_MS);
    });
    await waitFor(() => expect(screen.queryByText("Went back")).not.toBeInTheDocument());
  });
});
