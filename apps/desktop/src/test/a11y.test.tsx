import { screen } from "@testing-library/react";
import axe from "axe-core";
import { describe, expect, it } from "vitest";
import { useOnboarding } from "@/features/onboarding/store";
import { backend } from "@/test/backend";
import { defaultBackend, renderApp } from "@/test/render";

/** Runs axe on the page. Contrast is checked by `pnpm check:contrast`, jsdom has no layout. */
async function violations() {
  const result = await axe.run(document.body, {
    rules: { "color-contrast": { enabled: false }, region: { enabled: false } },
  });
  return result.violations.map(
    (v) => `${v.id}: ${v.nodes.map((n) => n.html.slice(0, 900)).join(" | ")}`,
  );
}

describe("accessibility", () => {
  it("Home has no violations", async () => {
    defaultBackend();
    renderApp("/");
    await screen.findByRole("main");
    expect(await violations()).toEqual([]);
  });

  it.each([
    "general",
    "appearance",
    "models",
    "git",
    "agents",
    "environment",
    "experimental",
    "advanced",
  ])("settings page %s has no violations", async (page) => {
    defaultBackend();
    renderApp(`/settings/${page}`);
    await screen.findByRole("navigation", { name: "Settings" });
    await screen.findAllByTestId("setting-row").catch(() => {});
    expect(await violations()).toEqual([]);
  });

  it("a repository page has no violations", async () => {
    defaultBackend();
    renderApp("/settings/repo/p1");
    await screen.findByLabelText("Setup script");
    expect(await violations()).toEqual([]);
  });

  it("the welcome dialog has no violations", async () => {
    defaultBackend();
    backend.on("project_list", []).on("dependency_check", [
      {
        name: "git",
        found: true,
        path: "/usr/bin/git",
        version: "2",
        ok: true,
        problem: null,
        fix: null,
      },
    ]);
    useOnboarding.setState({ done: false, armed: false });
    renderApp("/");
    await screen.findByText("Welcome to Circles");
    expect(await violations()).toEqual([]);
  });
});
