import type { UsageSummary } from "@circles/bindings/UsageSummary";
import { screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { backend } from "@/test/backend";
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
    // It sits in the sidebar footer, between the other footer buttons, where Add project was.
    const shortcuts = screen.getByRole("button", { name: "Keyboard shortcuts" });
    expect(usage.parentElement).toBe(shortcuts.parentElement);
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
});
