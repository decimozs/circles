import type { UsageSummary } from "@circles/bindings/UsageSummary";
import { screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { backend, emitBackendEvent } from "@/test/backend";
import { defaultBackend, renderApp } from "@/test/render";
import {
  compact,
  dayLabel,
  fillDays,
  folderName,
  money,
  niceCeiling,
  share,
  sinceFor,
} from "./format";

const NOW = Date.UTC(2026, 9, 31, 12);

function summary(over: Partial<UsageSummary> = {}): UsageSummary {
  return {
    sessions: 20,
    messages: 400,
    totals: {
      processed: 2_160_000_000,
      cached_input: 2_120_000_000,
      uncached_input: 15_900_000,
      output: 4_630_000,
      cache_savings_usd: 5272.01,
      cost_usd: 461.9,
      cost_input_usd: 0.04,
      cost_cache_read_usd: 277.48,
      cost_cache_write_usd: 127.44,
      cost_output_usd: 56.93,
    },
    by_day: [
      { day: "2026-10-30", cost_usd: 100, tokens: 1_000_000_000 },
      { day: "2026-10-31", cost_usd: 361.9, tokens: 1_160_000_000 },
    ],
    by_model: [
      {
        model: "claude-sonnet-5-5",
        cost_usd: 235.14,
        tokens: 1_490_000_000,
        messages: 300,
        priced: true,
      },
      { model: "claude-future-9", cost_usd: 0, tokens: 1_000, messages: 1, priced: false },
    ],
    folders: ["/Users/me/.claude", "/Users/me/.claude-work"],
    skipped_lines: 0,
    refreshed_at: Date.now() - 3 * 60_000,
    refreshing: false,
    files_done: 0,
    files_total: 0,
    ...over,
  };
}

async function openUsage() {
  const app = renderApp("/");
  await screen.findByRole("main");
  await app.user.click(screen.getByRole("button", { name: "Usage" }));
  return { ...app, dialog: await screen.findByRole("dialog") };
}

beforeEach(() => {
  defaultBackend();
  backend.on("usage_refresh", null).on("usage_rebuild", null);
  localStorage.clear();
});

describe("the numbers on the page", () => {
  it("writes money and token counts the short way", () => {
    expect(money(461.9)).toBe("$461.90");
    expect(money(5272.01)).toBe("$5,272.01");
    expect(money(0.001)).toBe("<$0.01");
    expect(money(0)).toBe("$0.00");
    expect(compact(2_160_000_000)).toBe("2.16B");
    expect(compact(15_900)).toBe("15.9K");
    expect(compact(4_630_000)).toBe("4.63M");
    expect(compact(812)).toBe("812");
    expect(share(509, 1000)).toBe("50.9%");
    expect(share(1, 10_000)).toBe("<0.1%");
    expect(share(0, 0)).toBe("0%");
    expect(dayLabel("2026-10-02")).toBe("OCT 2");
    expect(niceCeiling(87)).toBe(100);
    expect(niceCeiling(0)).toBe(1);
    expect(folderName("/Users/me/.claude-work")).toBe("~/.claude-work");
    expect(folderName("/elsewhere/claude")).toBe("/elsewhere/claude");
  });

  it("gives every day of the range an entry, with zero for the quiet ones", () => {
    const since = sinceFor("7d", NOW);
    const days = fillDays([{ day: "2026-10-30", cost_usd: 5, tokens: 10 }], since, NOW);
    expect(days.map((d) => d.day)).toEqual([
      "2026-10-24",
      "2026-10-25",
      "2026-10-26",
      "2026-10-27",
      "2026-10-28",
      "2026-10-29",
      "2026-10-30",
      "2026-10-31",
    ]);
    expect(days[6]?.cost_usd).toBe(5);
    expect(days[0]?.cost_usd).toBe(0);
  });
});

describe("the Usage dialog", () => {
  it("is opened by the footer button, which replaced Add project", async () => {
    backend.on("usage_summary", summary());
    renderApp("/");
    await screen.findByRole("main");
    const usage = screen.getByRole("button", { name: "Usage" });
    // The footer has Settings and Usage on the left, and Help alone on the right. The version
    // that used to be there is gone.
    const settings = screen.getByRole("button", { name: "Settings" });
    const help = screen.getByRole("button", { name: "Keyboard shortcuts" });
    expect(usage.parentElement).toBe(settings.parentElement);
    expect(help.parentElement).not.toBe(usage.parentElement);
    // Help is the last thing in the footer, on its own at the right.
    expect(help.parentElement?.lastElementChild).toBe(help);
    expect(usage.parentElement?.parentElement).toBe(help.parentElement);
    expect(screen.queryByText("v0.1.0")).not.toBeInTheDocument();
    // Settings comes first, then Usage.
    expect([...(usage.parentElement?.children ?? [])]).toEqual([settings, usage]);
  });

  it("shows the cost, the totals, the cost by type and the models", async () => {
    backend.on("usage_summary", summary());
    const { dialog } = await openUsage();
    const page = within(dialog);
    expect(await page.findByText("$461.90", { selector: "p" })).toBeInTheDocument();
    expect(page.getAllByText(/20 sessions/).length).toBeGreaterThan(0);
    expect(page.getByText("2.16B", { selector: "dd" })).toBeInTheDocument();
    expect(page.getByText("$5,272.01")).toBeInTheDocument();
    expect(page.getByText("Cost by type")).toBeInTheDocument();
    expect(page.getByText("$277.48")).toBeInTheDocument();
    expect(page.getByText("claude-sonnet-5-5")).toBeInTheDocument();
    expect(page.getByText("no price")).toBeInTheDocument();
    expect(page.getByText(/No price is known for claude-future-9/)).toBeInTheDocument();
    expect(page.getByRole("img", { name: /Daily cost/ })).toBeInTheDocument();
    // The query asks for the range the person last chose, 30 days by default.
    const call = backend.calls.find((c) => c.command === "usage_summary");
    expect(call?.args.folder).toBeNull();
    expect(typeof call?.args.sinceSecs).toBe("number");
  });

  it("asks again for the range and the environment that is picked, and remembers the range", async () => {
    backend.on("usage_summary", summary());
    const { dialog, user } = await openUsage();
    const page = within(dialog);
    await page.findByText("$461.90", { selector: "p" });
    await user.click(page.getByRole("button", { name: "7 days" }));
    await waitFor(() => expect(localStorage.getItem("circles.usage.range")).toBe("7d"));
    await user.selectOptions(page.getByLabelText("Environment"), "/Users/me/.claude-work");
    await waitFor(() =>
      expect(
        backend.calls.some(
          (c) => c.command === "usage_summary" && c.args.folder === "/Users/me/.claude-work",
        ),
      ).toBe(true),
    );
    expect(page.getByRole("option", { name: "~/.claude-work" })).toBeInTheDocument();
  });

  it("switches to tokens and groups the breakdown by day", async () => {
    backend.on("usage_summary", summary());
    const { dialog, user } = await openUsage();
    const page = within(dialog);
    await page.findByText("$461.90", { selector: "p" });
    await user.click(page.getByRole("button", { name: "Tokens" }));
    expect(page.getByRole("img", { name: /Daily tokens/ })).toBeInTheDocument();
    expect(page.getByText("Tokens by type")).toBeInTheDocument();
    await user.click(page.getByRole("button", { name: "day" }));
    expect(
      within(page.getByRole("region", { name: "Breakdown" })).getByText("OCT 31"),
    ).toBeInTheDocument();
  });

  it("says so when nothing was found, and where it looked", async () => {
    backend.on("usage_summary", summary({ messages: 0, sessions: 0 }));
    const { dialog } = await openUsage();
    expect(await within(dialog).findByText(/No Claude Code usage found/)).toBeInTheDocument();
    expect(within(dialog).getByText(/~\/\.claude, ~\/\.claude-work/)).toBeInTheDocument();
  });

  it("shows the plan limits Claude Code reported, or says there are none yet", async () => {
    backend.on("usage_summary", summary());
    backend.on("claude_rate_limit", null);
    const { dialog, user } = await openUsage();
    await user.click(within(dialog).getByRole("button", { name: "Limits" }));
    expect(await within(dialog).findByText(/No limit information yet/)).toBeInTheDocument();
  });

  it("shows the limits when there are some", async () => {
    backend.on("usage_summary", summary());
    backend.on("claude_rate_limit", {
      status: "allowed_warning",
      window: "five_hour",
      resets_at: 1_793_000_000,
      five_hour_utilization: 0.93,
      seven_day_utilization: 0.4,
      using_overage: false,
    });
    const { dialog, user } = await openUsage();
    await user.click(within(dialog).getByRole("button", { name: "Limits" }));
    expect(await within(dialog).findByText("Close to a limit")).toBeInTheDocument();
    expect(within(dialog).getByText("93% used")).toBeInTheDocument();
    expect(within(dialog).getByText("40% used")).toBeInTheDocument();
  });

  it("shows an error from the scan", async () => {
    backend.fail("usage_summary", { code: "internal", message: "usage scan stopped" });
    const { dialog } = await openUsage();
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("usage scan stopped");
  });

  it("opens with what is kept, says how old it is, and asks for a refresh in the background", async () => {
    backend.on("usage_summary", summary());
    const { dialog } = await openUsage();
    const page = within(dialog);
    // The numbers are there at once, with their age; nothing says it is loading.
    expect(await page.findByText("$461.90", { selector: "p" })).toBeInTheDocument();
    expect(page.getByText("Updated 3min ago")).toBeInTheDocument();
    expect(page.queryByText(/Reading/)).not.toBeInTheDocument();
    expect(backend.calls.some((c) => c.command === "usage_refresh")).toBe(true);
  });

  it("shows the kept numbers while a refresh runs, and reads again when it adds something", async () => {
    let calls = 0;
    backend.on("usage_summary", () => {
      calls += 1;
      return calls === 1
        ? summary({ refreshing: true, files_done: 4, files_total: 9 })
        : summary({
            refreshing: false,
            totals: { ...summary().totals, cost_usd: 470.5 },
          });
    });
    const { dialog } = await openUsage();
    const page = within(dialog);
    expect(await page.findByText("$461.90", { selector: "p" })).toBeInTheDocument();
    expect(page.getByText(/reading 4 of 9 files/)).toBeInTheDocument();
    emitBackendEvent("usage:changed", null);
    expect(await page.findByText("$470.50", { selector: "p" })).toBeInTheDocument();
    expect(page.queryByText(/reading 4 of 9 files/)).not.toBeInTheDocument();
  });

  it("keeps the numbers on screen while another range is read", async () => {
    let release: (value: UsageSummary) => void = () => {};
    backend.on("usage_summary", (args: Record<string, unknown>) => {
      // The first answer is at once; the one for the range chosen later waits.
      if (backend.calls.filter((c) => c.command === "usage_summary").length <= 1) return summary();
      void args;
      return new Promise<UsageSummary>((resolve) => {
        release = resolve;
      });
    });
    const { dialog, user } = await openUsage();
    const page = within(dialog);
    await page.findByText("$461.90", { selector: "p" });
    await user.click(page.getByRole("button", { name: "7 days" }));
    await waitFor(() =>
      expect(backend.calls.filter((c) => c.command === "usage_summary").length).toBeGreaterThan(1),
    );
    // No loading screen replaces what was shown.
    expect(page.getByText("$461.90", { selector: "p" })).toBeInTheDocument();
    expect(page.queryByText("Loading…")).not.toBeInTheDocument();
    release(summary({ totals: { ...summary().totals, cost_usd: 12.34 } }));
    expect(await page.findByText("$12.34", { selector: "p" })).toBeInTheDocument();
  });

  it("shows progress, not an empty page, the first time the history is read", async () => {
    backend.on(
      "usage_summary",
      summary({
        messages: 0,
        sessions: 0,
        refreshed_at: null,
        refreshing: true,
        files_done: 312,
        files_total: 971,
      }),
    );
    const { dialog } = await openUsage();
    expect(
      await within(dialog).findByText(
        /Reading Claude Code's history for the first time: 312 of 971 files/,
      ),
    ).toBeInTheDocument();
    expect(within(dialog).queryByText(/No Claude Code usage found/)).not.toBeInTheDocument();
  });

  it("asks before it forgets what was kept, and rebuilds when told to", async () => {
    backend.on("usage_summary", summary());
    const { dialog, user } = await openUsage();
    const page = within(dialog);
    await page.findByText("$461.90", { selector: "p" });
    await user.click(page.getByRole("button", { name: "Rebuild history" }));
    expect(page.getByText(/cannot come back/)).toBeInTheDocument();
    expect(backend.calls.some((c) => c.command === "usage_rebuild")).toBe(false);
    await user.click(page.getByRole("button", { name: "Cancel" }));
    expect(page.getByRole("button", { name: "Rebuild history" })).toBeInTheDocument();
    await user.click(page.getByRole("button", { name: "Rebuild history" }));
    await user.click(page.getByRole("button", { name: "Rebuild" }));
    await waitFor(() =>
      expect(backend.calls.some((c) => c.command === "usage_rebuild")).toBe(true),
    );
  });
});
