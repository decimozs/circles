import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { backend, emitBackendEvent } from "@/test/backend";
import { defaultBackend, renderApp, settingsBackend } from "@/test/render";

function open() {
  defaultBackend();
  settingsBackend({});
  return renderApp("/settings/account");
}

describe("account cards", () => {
  it("shows the GitHub and Claude accounts as cards, like the Jira card", async () => {
    open();
    const github = await screen.findByRole("region", { name: "GitHub" });
    expect(await within(github).findByTestId("github-state")).toHaveTextContent("Connected");
    expect(within(github).getByTestId("github-details")).toHaveTextContent("@me");
    const claude = await screen.findByRole("region", { name: "Claude" });
    expect(await within(claude).findByTestId("claude-state")).toHaveTextContent("Connected");
    expect(within(claude).getByTestId("claude-details")).toHaveTextContent(
      "me@example.com · Claude Pro",
    );
    // The old placeholder is gone.
    expect(screen.queryByText(/Sign in arrives with accounts/)).not.toBeInTheDocument();
  });

  it("offers the other GitHub accounts and switches to the one picked", async () => {
    defaultBackend();
    settingsBackend({});
    backend
      .on("github_accounts", {
        installed: true,
        accounts: [
          { login: "work", active: true },
          { login: "me", active: false },
        ],
        fix: null,
      })
      .on("github_switch_account", {
        installed: true,
        accounts: [
          { login: "me", active: true },
          { login: "work", active: false },
        ],
        fix: null,
      });
    const app = renderApp("/settings/account");
    const github = await screen.findByRole("region", { name: "GitHub" });
    expect(await within(github).findByTestId("github-details")).toHaveTextContent(
      "@work · 1 more signed in",
    );
    await app.user.click(within(github).getByRole("button", { name: /Switch account/ }));
    await app.user.click(await screen.findByRole("menuitem", { name: "@me" }));
    await waitFor(() =>
      expect(backend.callsTo("github_switch_account")).toEqual([{ login: "me" }]),
    );
    await waitFor(() =>
      expect(within(github).getByTestId("github-details")).toHaveTextContent("@me"),
    );
  });

  it("says what to run when GitHub or Claude is not signed in", async () => {
    defaultBackend();
    settingsBackend({});
    backend
      .on("github_accounts", { installed: true, accounts: [], fix: "gh auth login" })
      .on("claude_account", {
        found: true,
        signed_in: false,
        email: null,
        method: null,
        plan: null,
        fix: "claude",
      });
    renderApp("/settings/account");
    const github = await screen.findByRole("region", { name: "GitHub" });
    expect(await within(github).findByTestId("github-state")).toHaveTextContent("Not signed in");
    expect(within(github).getByText("gh auth login")).toBeInTheDocument();
    expect(
      within(github).queryByRole("button", { name: /Switch account/ }),
    ).not.toBeInTheDocument();
    const claude = await screen.findByRole("region", { name: "Claude" });
    expect(await within(claude).findByTestId("claude-state")).toHaveTextContent("Not signed in");
    expect(within(claude).getByText("claude")).toBeInTheDocument();
  });

  it("says when the GitHub CLI or Claude Code is not installed", async () => {
    defaultBackend();
    settingsBackend({});
    backend
      .on("github_accounts", { installed: false, accounts: [], fix: "brew install gh" })
      .on("claude_account", {
        found: false,
        signed_in: false,
        email: null,
        method: null,
        plan: null,
        fix: null,
      });
    renderApp("/settings/account");
    const github = await screen.findByRole("region", { name: "GitHub" });
    expect(await within(github).findByTestId("github-state")).toHaveTextContent("Not installed");
    expect(within(github).getByText("brew install gh")).toBeInTheDocument();
    const claude = await screen.findByRole("region", { name: "Claude" });
    expect(await within(claude).findByTestId("claude-state")).toHaveTextContent("Not installed");
  });

  it("shows the account that was kept at once and changes when it is asked again in the background", async () => {
    let calls = 0;
    defaultBackend();
    settingsBackend({});
    backend.on("github_accounts", () => {
      calls += 1;
      return {
        installed: true,
        accounts: [{ login: calls === 1 ? "kept-account" : "new-account", active: true }],
        fix: null,
      };
    });
    renderApp("/settings/account");
    const github = await screen.findByRole("region", { name: "GitHub" });
    expect(await within(github).findByTestId("github-details")).toHaveTextContent("@kept-account");
    emitBackendEvent("cache:changed", "github-accounts");
    await waitFor(() =>
      expect(within(github).getByTestId("github-details")).toHaveTextContent("@new-account"),
    );
  });
});
