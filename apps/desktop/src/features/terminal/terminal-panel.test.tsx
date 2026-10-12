import { act, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useUi } from "@/stores/ui";
import { backend, emitBackendEvent, type FakeChannel } from "@/test/backend";
import { defaultTabs, scripts, terminal, workspace } from "@/test/fixtures";
import { defaultBackend, renderApp } from "@/test/render";
import { FakeSearchAddon, FakeTerminal } from "@/test/xterm-mock";

const web = {
  name: "web",
  command: "pnpm dev --port $CIRCLES_PORT",
  default: true,
  icon: "server",
};
const tests = {
  name: "tests",
  command: "pnpm test --watch",
  default: false,
  icon: "flask-conical",
};

type Options = {
  tabs?: ReturnType<typeof defaultTabs>;
  scriptsInfo?: ReturnType<typeof scripts>;
  ws?: Partial<ReturnType<typeof workspace>>;
  /** The terminal starts closed. Most tests need it open. */
  dockOpen?: boolean;
};

async function openWorkspace({
  tabs = defaultTabs(),
  scriptsInfo = scripts(),
  ws = {},
  dockOpen = true,
}: Options = {}) {
  defaultBackend();
  backend
    .on("workspace_get", workspace({ id: "w1", city: "Algiers", ...ws }))
    .on("terminal_list", tabs)
    .on("scripts_get", scriptsInfo);
  const app = renderApp("/workspace/w1", { dockOpen });
  const panel = await screen.findByRole("region", { name: "Terminal" });
  await within(panel).findByRole("tablist", { name: "Terminal tabs" });
  return { ...app, panel: () => screen.getByRole("region", { name: "Terminal" }) };
}

const tabNamed = (panel: HTMLElement, name: string) =>
  within(panel).getByRole("tab", { name: new RegExp(`^${name}`) });

describe("Terminal panel", () => {
  it("shows the Setup, Run and Terminal tabs with the shell selected", async () => {
    const { panel } = await openWorkspace();
    const tabs = within(panel())
      .getAllByRole("tab")
      .map((t) => t.textContent);
    expect(tabs).toEqual(["Setup", "Run", "Terminal"]);
    expect(tabNamed(panel(), "Terminal")).toHaveAttribute("aria-selected", "true");
    await waitFor(() => expect(backend.callsTo("terminal_attach")).toHaveLength(1));
    expect(backend.callsTo("terminal_attach")[0]).toMatchObject({
      id: "t-term",
      cols: 100,
      rows: 28,
    });
  });

  it("writes output to the terminal and sends typing, resizes and exit back", async () => {
    await openWorkspace();
    await waitFor(() => expect(backend.callsTo("terminal_attach")).toHaveLength(1));
    const term = FakeTerminal.last;
    // The chat attaches its own channel too, so take the terminal's.
    const channel = backend.callsTo("terminal_attach")[0].onEvent as FakeChannel;

    act(() => channel.emit(new TextEncoder().encode("hello from the shell\r\n")));
    expect(term.text).toContain("hello from the shell");

    term.type("ls -la\r");
    await waitFor(() =>
      expect(backend.callsTo("terminal_write")).toEqual([{ id: "t-term", data: "ls -la\r" }]),
    );

    term.resize(120, 40);
    await waitFor(() =>
      expect(backend.callsTo("terminal_resize")).toEqual([{ id: "t-term", cols: 120, rows: 40 }]),
    );

    act(() => channel.emit({ type: "exit", code: 3, signaled: false }));
    expect(term.text).toContain("Process exited with code 3");
    act(() => channel.emit({ type: "exit", code: 0, signaled: false }));
    expect(term.text).toContain("Process finished");
    act(() => channel.emit({ type: "exit", code: null, signaled: true }));
    expect(term.text).toContain("Process was stopped");

    act(() => channel.emit({ type: "reset" }));
    expect(term.resets).toBe(1);
  });

  it("uses the design tokens for the terminal colors", async () => {
    await openWorkspace();
    await waitFor(() => expect(FakeTerminal.instances).toHaveLength(1));
    const theme = FakeTerminal.last.options.theme as Record<string, unknown>;
    expect(theme).toHaveProperty("background");
    expect(theme).toHaveProperty("brightWhite");
    expect(theme).toHaveProperty("selectionBackground");
  });

  it("disconnects a tab when switching away and reconnects when coming back", async () => {
    const { user, panel } = await openWorkspace({
      tabs: defaultTabs({ run: { has_output: true } }),
    });
    await waitFor(() => expect(backend.callsTo("terminal_attach")).toHaveLength(1));

    await user.click(tabNamed(panel(), "Run"));
    await waitFor(() => expect(backend.callsTo("terminal_detach")).toEqual([{ id: "t-term" }]));
    await waitFor(() => expect(backend.callsTo("terminal_attach")).toHaveLength(2));
    expect(backend.callsTo("terminal_attach")[1]).toMatchObject({ id: "t-run" });
    expect(FakeTerminal.instances[0].disposed).toBe(true);

    await user.click(tabNamed(panel(), "Terminal"));
    await waitFor(() => expect(backend.callsTo("terminal_attach")).toHaveLength(3));
    expect(backend.callsTo("terminal_attach")[2]).toMatchObject({ id: "t-term" });
  });

  it("searches the terminal output with Cmd+F", async () => {
    const { user } = await openWorkspace();
    await waitFor(() => expect(FakeTerminal.instances).toHaveLength(1));
    FakeSearchAddon.searches = [];
    let handled = true;
    act(() => {
      handled = FakeTerminal.last.press({ metaKey: true, key: "f" });
    });
    expect(handled).toBe(false);
    const input = await screen.findByLabelText("Find in terminal");
    await user.type(input, "error");
    expect(FakeSearchAddon.searches).toContain("next:error");
    await user.click(screen.getByRole("button", { name: "Previous match" }));
    expect(FakeSearchAddon.searches).toContain("prev:error");
    await user.keyboard("{Escape}");
    await waitFor(() =>
      expect(screen.queryByLabelText("Find in terminal")).not.toBeInTheDocument(),
    );
  });

  describe("Run tab", () => {
    it("offers to start the default script, and the others", async () => {
      backend.on("script_run", terminal({ id: "t-run", kind: "run", running: true }));
      const { user, panel } = await openWorkspace({ scriptsInfo: scripts({ run: [web, tests] }) });
      await user.click(tabNamed(panel(), "Run"));
      const start = await screen.findByRole("button", { name: /Start web/ });
      expect(start).toHaveTextContent("⌘R");
      expect(screen.getByText("Test your changes here.")).toBeInTheDocument();

      await user.click(start);
      await waitFor(() =>
        expect(backend.callsTo("script_run")).toEqual([{ workspaceId: "w1", name: null }]),
      );

      await user.click(screen.getByRole("button", { name: "tests" }));
      await waitFor(() =>
        expect(backend.callsTo("script_run").pop()).toEqual({ workspaceId: "w1", name: "tests" }),
      );
    });

    it("explains how to add a run script when there is none", async () => {
      backend.on("project_open_settings", "/code/roster/.circles/settings.toml");
      const { user, panel } = await openWorkspace();
      await user.click(tabNamed(panel(), "Run"));
      expect(await screen.findByText("No run script yet.")).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: /Open \.circles\/settings\.toml/ }));
      await waitFor(() =>
        expect(backend.callsTo("project_open_settings")).toEqual([{ projectId: "p1" }]),
      );
    });

    it("shows the output, Stop and the script menu while a script runs", async () => {
      backend.on(
        "script_stop",
        terminal({ id: "t-run", kind: "run", running: false, has_output: true }),
      );
      const { user, panel } = await openWorkspace({
        tabs: defaultTabs({ run: { running: true, has_output: true, script: "web" } }),
        scriptsInfo: scripts({ run: [web, tests] }),
      });
      await user.click(tabNamed(panel(), "Run"));
      await waitFor(() =>
        expect(backend.callsTo("terminal_attach").pop()).toMatchObject({ id: "t-run" }),
      );
      expect(within(tabNamed(panel(), "Run")).getByLabelText("Running")).toBeInTheDocument();

      await user.click(screen.getByRole("button", { name: "Stop" }));
      await waitFor(() => expect(backend.callsTo("script_stop")).toEqual([{ workspaceId: "w1" }]));
    });

    it("runs a chosen script from the menu, and shows which one is the default", async () => {
      backend.on("script_run", terminal({ id: "t-run", kind: "run", running: true }));
      const { user, panel } = await openWorkspace({
        tabs: defaultTabs({ run: { has_output: true } }),
        scriptsInfo: scripts({ run: [web, tests] }),
      });
      await user.click(tabNamed(panel(), "Run"));
      await user.click(await screen.findByRole("button", { name: "Run scripts" }));
      const defaultItem = await screen.findByRole("menuitem", { name: /web/ });
      expect(defaultItem).toHaveTextContent("default");
      await user.click(screen.getByRole("menuitem", { name: /tests/ }));
      await waitFor(() =>
        expect(backend.callsTo("script_run")).toEqual([{ workspaceId: "w1", name: "tests" }]),
      );
    });

    it("runs the default script from the Run button", async () => {
      backend.on("script_run", terminal({ id: "t-run", kind: "run", running: true }));
      const { user, panel } = await openWorkspace({
        tabs: defaultTabs({ run: { has_output: true } }),
        scriptsInfo: scripts({ run: [web] }),
      });
      await user.click(tabNamed(panel(), "Run"));
      await user.click(await screen.findByRole("button", { name: "Run web" }));
      await waitFor(() =>
        expect(backend.callsTo("script_run")).toEqual([{ workspaceId: "w1", name: null }]),
      );
    });

    it("marks a run that failed", async () => {
      const { panel } = await openWorkspace({
        tabs: defaultTabs({ run: { has_output: true, exit: { code: 1, signaled: false } } }),
      });
      expect(within(tabNamed(panel(), "Run")).getByLabelText("Failed")).toBeInTheDocument();
    });
  });

  describe("Setup tab", () => {
    it("offers to run a configured setup script", async () => {
      backend.on("script_run_setup", terminal({ id: "t-setup", kind: "setup", running: true }));
      const { user, panel } = await openWorkspace({
        scriptsInfo: scripts({ setup: "pnpm install" }),
      });
      await user.click(tabNamed(panel(), "Setup"));
      expect(await screen.findByText("pnpm install")).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: "Run setup" }));
      await waitFor(() =>
        expect(backend.callsTo("script_run_setup")).toEqual([{ workspaceId: "w1" }]),
      );
    });

    it("explains setup scripts when none is configured", async () => {
      const { user, panel } = await openWorkspace();
      await user.click(tabNamed(panel(), "Setup"));
      expect(await screen.findByText("No setup script yet.")).toBeInTheDocument();
      expect(screen.getByText("pnpm install")).toBeInTheDocument();
    });

    it("shows the output and a Run again button once it has run", async () => {
      const { user, panel } = await openWorkspace({
        tabs: defaultTabs({ setup: { has_output: true, exit: { code: 0, signaled: false } } }),
        scriptsInfo: scripts({ setup: "pnpm install" }),
      });
      await user.click(tabNamed(panel(), "Setup"));
      await waitFor(() =>
        expect(backend.callsTo("terminal_attach").pop()).toMatchObject({ id: "t-setup" }),
      );
      expect(screen.getByRole("button", { name: "Run again" })).toBeEnabled();
    });

    it("jumps to the Setup tab while the setup script runs", async () => {
      await openWorkspace({
        ws: { state: "setup_running" },
        tabs: defaultTabs({ setup: { running: true, has_output: true } }),
        scriptsInfo: scripts({ setup: "pnpm install" }),
      });
      await waitFor(() =>
        expect(backend.callsTo("terminal_attach").pop()).toMatchObject({ id: "t-setup" }),
      );
      expect(useUi.getState().dockOpen).toBe(true);
    });
  });

  describe("tabs", () => {
    it("adds a terminal and selects it", async () => {
      backend.on("terminal_create", terminal({ id: "t-new", title: "Terminal 2", position: 3 }));
      const { user, panel } = await openWorkspace();
      backend.on("terminal_list", [
        ...defaultTabs(),
        terminal({ id: "t-new", title: "Terminal 2", position: 3 }),
      ]);
      await user.click(within(panel()).getByRole("button", { name: "New terminal" }));
      await waitFor(() =>
        expect(backend.callsTo("terminal_create")).toEqual([{ workspaceId: "w1", title: null }]),
      );
      expect(await within(panel()).findByRole("tab", { name: /Terminal 2/ })).toHaveAttribute(
        "aria-selected",
        "true",
      );
    });

    it("closes a free terminal but not Setup or Run", async () => {
      backend.on("terminal_delete", null);
      const { user, panel } = await openWorkspace();
      expect(
        within(panel()).queryByRole("button", { name: "Close Setup" }),
      ).not.toBeInTheDocument();
      expect(within(panel()).queryByRole("button", { name: "Close Run" })).not.toBeInTheDocument();
      await user.click(within(panel()).getByRole("button", { name: "Close Terminal" }));
      await waitFor(() => expect(backend.callsTo("terminal_delete")).toEqual([{ id: "t-term" }]));
    });

    it("shows a script that starts or ends once the backend says so", async () => {
      const { panel } = await openWorkspace();
      expect(within(tabNamed(panel(), "Run")).queryByLabelText("Running")).not.toBeInTheDocument();
      backend.on("terminal_list", defaultTabs({ run: { running: true, has_output: true } }));
      act(() =>
        emitBackendEvent("terminal:changed", {
          terminal_id: "t-run",
          workspace_id: "w1",
          running: true,
        }),
      );
      expect(await within(tabNamed(panel(), "Run")).findByLabelText("Running")).toBeInTheDocument();

      backend.on(
        "terminal_list",
        defaultTabs({
          run: { running: false, has_output: true, exit: { code: 0, signaled: false } },
        }),
      );
      act(() =>
        emitBackendEvent("terminal:changed", {
          terminal_id: "t-run",
          workspace_id: "w1",
          running: false,
        }),
      );
      await waitFor(() =>
        expect(
          within(tabNamed(panel(), "Run")).queryByLabelText("Running"),
        ).not.toBeInTheDocument(),
      );
    });

    it("never shows a spinner on a free terminal, even though its shell is running", async () => {
      const { panel } = await openWorkspace({
        tabs: defaultTabs({ terminal: { running: true, has_output: true } }),
      });
      expect(
        within(tabNamed(panel(), "Terminal")).queryByLabelText("Running"),
      ).not.toBeInTheDocument();
      expect(within(tabNamed(panel(), "Terminal")).queryByRole("img")).not.toBeInTheDocument();
    });

    it("hides the terminal with the arrow, and brings it back with the top bar button", async () => {
      const { user, panel } = await openWorkspace();
      await user.click(within(panel()).getByRole("button", { name: "Hide terminal" }));
      expect(useUi.getState().dockOpen).toBe(false);
      await waitFor(() =>
        expect(screen.queryByRole("region", { name: "Terminal" })).not.toBeInTheDocument(),
      );
      expect(screen.queryByTestId("terminal-host")).not.toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: "Toggle terminal" }));
      expect(await screen.findByTestId("terminal-host")).toBeInTheDocument();
    });

    it("starts closed, and opens from the button in the top bar", async () => {
      defaultBackend();
      backend.on("workspace_get", workspace({ id: "w1", city: "Algiers" }));
      const { user } = renderApp("/workspace/w1");
      const button = await screen.findByRole("button", { name: "Toggle terminal" });
      expect(button).toHaveAttribute("aria-pressed", "false");
      expect(screen.queryByRole("region", { name: "Terminal" })).not.toBeInTheDocument();
      expect(backend.callsTo("terminal_attach")).toHaveLength(0);

      await user.click(button);
      expect(await screen.findByRole("region", { name: "Terminal" })).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Toggle terminal" })).toHaveAttribute(
        "aria-pressed",
        "true",
      );
      await user.click(screen.getByRole("button", { name: "Toggle terminal" }));
      await waitFor(() =>
        expect(screen.queryByRole("region", { name: "Terminal" })).not.toBeInTheDocument(),
      );
    });

    it("sits under the chat in the center column, not in the right pane", async () => {
      const { panel } = await openWorkspace();
      const rightPane = screen.getByRole("tablist", { name: "Review" })
        .parentElement as HTMLElement;
      expect(rightPane).not.toContainElement(panel());
      const center = screen.getByRole("main").parentElement?.parentElement as HTMLElement;
      expect(center).toContainElement(panel());
      expect(center).toContainElement(screen.getByRole("region", { name: "Chat" }));
    });

    it("opens by itself when the setup script starts, even when closed", async () => {
      defaultBackend();
      backend
        .on("workspace_get", workspace({ id: "w1", city: "Algiers", state: "setup_running" }))
        .on("terminal_list", defaultTabs({ setup: { running: true, has_output: true } }))
        .on("scripts_get", scripts({ setup: "pnpm install" }));
      renderApp("/workspace/w1");
      expect(await screen.findByRole("region", { name: "Terminal" })).toBeInTheDocument();
      expect(useUi.getState().dockOpen).toBe(true);
    });

    it("has no terminal for an archived workspace", async () => {
      defaultBackend();
      backend.on("workspace_get", workspace({ id: "w1", state: "archived" }));
      renderApp("/workspace/w1");
      await screen.findByText(/Archived\./);
      expect(screen.queryByRole("region", { name: "Terminal" })).not.toBeInTheDocument();
    });
  });

  describe("the layout", () => {
    it("clips the whole app, so nothing makes the window scroll", async () => {
      const { panel } = await openWorkspace();
      const root = panel().closest(".overflow-clip");
      expect(root).not.toBeNull();
      expect(root).toContainElement(screen.getByRole("region", { name: "Chat" }));
    });
  });

  describe("opening", () => {
    const dockSize = () =>
      (document.querySelector('[data-panel-id="dock"]') as HTMLElement).dataset.size;
    const closed = async (tabs = defaultTabs()) => {
      defaultBackend();
      backend
        .on("workspace_get", workspace({ id: "w1", city: "Algiers" }))
        .on("terminal_list", tabs);
      const app = renderApp("/workspace/w1");
      await screen.findByRole("button", { name: "Toggle terminal" });
      return app;
    };
    const expected = () => `${Math.round(window.innerHeight * 0.3)}px`;

    it("opens at 30% of the window with the button, and the same with the shortcut", async () => {
      const app = await closed();
      await app.user.click(screen.getByRole("button", { name: "Toggle terminal" }));
      await screen.findByRole("region", { name: "Terminal" });
      expect(dockSize()).toBe(expected());
      expect(useUi.getState().bigTerminal).toBe(false);

      await app.user.keyboard("{Meta>}j{/Meta}"); // hide
      await waitFor(() =>
        expect(screen.queryByRole("region", { name: "Terminal" })).not.toBeInTheDocument(),
      );
      await app.user.keyboard("{Meta>}j{/Meta}"); // show again
      await screen.findByRole("region", { name: "Terminal" });
      expect(dockSize()).toBe(expected());
      expect(useUi.getState().bigTerminal).toBe(false);
    });

    it("opens at the height the user dragged it to, by either way", async () => {
      const app = await closed();
      useUi.getState().setDockHeight(410);
      await app.user.keyboard("{Meta>}j{/Meta}");
      await screen.findByRole("region", { name: "Terminal" });
      expect(dockSize()).toBe("410px");
    });

    it("never opens taller than the window leaves room for", async () => {
      const app = await closed();
      useUi.getState().setDockHeight(5000);
      await app.user.click(screen.getByRole("button", { name: "Toggle terminal" }));
      await screen.findByRole("region", { name: "Terminal" });
      expect(Number.parseFloat(dockSize() ?? "")).toBeLessThanOrEqual(window.innerHeight - 200);
    });

    it("scrolls rather than spills when it is short", async () => {
      const { panel } = await openWorkspace();
      const host = within(panel()).getByTestId("terminal-host");
      expect(host.closest(".overflow-y-auto")).not.toBeNull();
    });

    it("makes a terminal when it opens and there is none", async () => {
      backend.on("terminal_create", terminal({ id: "t-made", title: "Terminal 2", position: 3 }));
      const app = await closed(defaultTabs().filter((t) => t.kind !== "terminal"));
      await app.user.click(screen.getByRole("button", { name: "Toggle terminal" }));
      await waitFor(() => expect(backend.callsTo("terminal_create")).toHaveLength(1));
      await waitFor(() => expect(useUi.getState().dockTab.w1).toBe("t-made"));
    });

    it("shows the terminal that is already there, and does not make another", async () => {
      const app = await closed();
      await app.user.click(screen.getByRole("button", { name: "Toggle terminal" }));
      const panel = await screen.findByRole("region", { name: "Terminal" });
      expect(within(panel).getByRole("tab", { name: /^Terminal/ })).toHaveAttribute(
        "aria-selected",
        "true",
      );
      await new Promise((resolve) => setTimeout(resolve, 100));
      expect(backend.callsTo("terminal_create")).toHaveLength(0);
    });
  });

  describe("when the shell ends", () => {
    const exited = { exit: { code: 0, signaled: false }, has_output: true };

    it("gives the last terminal a new, clean shell", async () => {
      backend.on("terminal_restart", null);
      await openWorkspace({ tabs: defaultTabs({ terminal: exited }) });
      await waitFor(() => expect(backend.callsTo("terminal_restart")).toEqual([{ id: "t-term" }]));
      // The view is made again, so the screen starts clean.
      await waitFor(() =>
        expect(backend.callsTo("terminal_attach").length).toBeGreaterThanOrEqual(2),
      );
    });

    it("closes an extra terminal instead", async () => {
      backend.on("terminal_delete", null);
      await openWorkspace({
        tabs: [
          ...defaultTabs(),
          terminal({ id: "t-second", title: "Terminal 2", position: 3, ...exited }),
        ],
      });
      // The extra, ended one is the open tab.
      useUi.getState().selectDockTab("w1", "t-second");
      await waitFor(() => expect(backend.callsTo("terminal_delete")).toEqual([{ id: "t-second" }]));
      expect(backend.callsTo("terminal_restart")).toHaveLength(0);
    });

    it("leaves a script tab alone when its script has finished", async () => {
      await openWorkspace({
        tabs: defaultTabs({ run: { exit: { code: 0, signaled: false }, has_output: true } }),
      });
      useUi.getState().selectDockTab("w1", "t-run");
      await new Promise((resolve) => setTimeout(resolve, 100));
      expect(backend.callsTo("terminal_restart")).toHaveLength(0);
      expect(backend.callsTo("terminal_delete")).toHaveLength(0);
    });

    it("does nothing for a terminal that is still running", async () => {
      await openWorkspace({ tabs: defaultTabs({ terminal: { running: true, has_output: true } }) });
      await new Promise((resolve) => setTimeout(resolve, 100));
      expect(backend.callsTo("terminal_restart")).toHaveLength(0);
    });
  });

  describe("maximising", () => {
    const pagePanel = () => document.querySelector('[data-panel-id="page"]') as HTMLElement;

    it("gives the terminal the whole column, with no second terminal and the chat kept", async () => {
      const { user, panel } = await openWorkspace();
      expect(pagePanel().dataset.collapsed).not.toBe("true");
      await user.click(within(panel()).getByRole("button", { name: "Maximize terminal" }));
      expect(useUi.getState().bigTerminal).toBe(true);

      // Still one terminal, not another one in the page.
      expect(screen.getAllByRole("region", { name: "Terminal" })).toHaveLength(1);
      expect(
        within(screen.getByRole("main")).queryByRole("region", { name: "Terminal" }),
      ).not.toBeInTheDocument();
      expect(screen.queryByText(/open in the center/)).not.toBeInTheDocument();
      // The chat is out of the way but still there, so it keeps running.
      expect(pagePanel().dataset.collapsed).toBe("true");
      expect(screen.getByRole("region", { name: "Chat" })).toBeInTheDocument();
      expect(backend.callsTo("terminal_attach")).toHaveLength(1);

      await user.click(within(panel()).getByRole("button", { name: "Restore terminal size" }));
      expect(useUi.getState().bigTerminal).toBe(false);
      expect(pagePanel().dataset.collapsed).toBe("false");
      expect(backend.callsTo("terminal_attach")).toHaveLength(1);
    });

    it("opens the terminal when it is maximised while hidden", async () => {
      defaultBackend();
      backend.on("workspace_get", workspace({ id: "w1", city: "Algiers" }));
      const { user } = renderApp("/workspace/w1");
      await screen.findByRole("button", { name: "Toggle terminal" });
      expect(screen.queryByRole("region", { name: "Terminal" })).not.toBeInTheDocument();
      // Maximising is the button, the palette or Cmd+Shift+M.
      act(() => useUi.getState().toggleBigTerminal());
      expect(useUi.getState().dockOpen).toBe(true);
      expect(useUi.getState().bigTerminal).toBe(true);
      expect(await screen.findByRole("region", { name: "Terminal" })).toBeInTheDocument();
      expect(pagePanel().dataset.collapsed).toBe("true");
      expect(user).toBeDefined();
    });

    it("gives the column back when the terminal is hidden", async () => {
      const { user, panel } = await openWorkspace();
      await user.click(within(panel()).getByRole("button", { name: "Maximize terminal" }));
      await user.click(within(panel()).getByRole("button", { name: "Hide terminal" }));
      expect(useUi.getState().dockOpen).toBe(false);
      expect(useUi.getState().bigTerminal).toBe(false);
      await waitFor(() => expect(pagePanel().dataset.collapsed).toBe("false"));
    });
  });

  describe("shortcuts", () => {
    it("runs the default script with Cmd+R and shows the Run tab", async () => {
      backend.on("script_run", terminal({ id: "t-run", kind: "run", running: true }));
      const { user } = await openWorkspace({ scriptsInfo: scripts({ run: [web] }) });
      await user.keyboard("{Meta>}r{/Meta}");
      await waitFor(() =>
        expect(backend.callsTo("script_run")).toEqual([{ workspaceId: "w1", name: null }]),
      );
      await waitFor(() => expect(useUi.getState().dockTab.w1).toBe("t-run"));
    });

    it("tells the user when there is nothing to run", async () => {
      backend.fail("script_run", { code: "invalid", message: "This project has no run script." });
      const { user } = await openWorkspace();
      await user.keyboard("{Meta>}r{/Meta}");
      expect(await screen.findByText("This project has no run script.")).toBeInTheDocument();
    });

    it("stops with Cmd+Shift+R", async () => {
      backend.on("script_stop", terminal({ id: "t-run", kind: "run" }));
      const { user } = await openWorkspace();
      await user.keyboard("{Meta>}{Shift>}r{/Shift}{/Meta}");
      await waitFor(() => expect(backend.callsTo("script_stop")).toEqual([{ workspaceId: "w1" }]));
    });

    it("opens a new terminal with Cmd+T", async () => {
      backend.on("terminal_create", terminal({ id: "t-new", title: "Terminal 2" }));
      const { user } = await openWorkspace();
      await user.keyboard("{Meta>}t{/Meta}");
      await waitFor(() => expect(backend.callsTo("terminal_create")).toHaveLength(1));
      await waitFor(() => expect(useUi.getState().dockTab.w1).toBe("t-new"));
    });

    it("opens and closes the terminal with Cmd+J, and with Cmd+Shift+J, and does nothing else", async () => {
      const { user } = await openWorkspace();
      for (const press of ["{Meta>}j{/Meta}", "{Meta>}{Shift>}j{/Shift}{/Meta}"]) {
        await user.keyboard(press);
        expect(useUi.getState().dockOpen).toBe(false);
        await user.keyboard(press);
        expect(useUi.getState().dockOpen).toBe(true);
      }
      expect(useUi.getState().bigTerminal).toBe(false);
    });

    it("keeps alternating between open and closed when the key is pressed again and again", async () => {
      const { user } = await openWorkspace();
      const seen: boolean[] = [];
      for (let i = 0; i < 8; i++) {
        await user.keyboard("{Meta>}{Shift>}j{/Shift}{/Meta}");
        seen.push(useUi.getState().dockOpen);
        expect(useUi.getState().bigTerminal).toBe(false);
      }
      expect(seen).toEqual([false, true, false, true, false, true, false, true]);
    });

    it("maximises from the button, and the J key only opens and closes", async () => {
      const { user, panel } = await openWorkspace();
      await user.click(within(panel()).getByRole("button", { name: "Maximize terminal" }));
      expect(useUi.getState().bigTerminal).toBe(true);
      // The key hides the terminal, and a hidden terminal is not maximised.
      await user.keyboard("{Meta>}{Shift>}j{/Shift}{/Meta}");
      expect(useUi.getState().dockOpen).toBe(false);
      expect(useUi.getState().bigTerminal).toBe(false);
    });

    it("maximises and restores with Cmd+Shift+M", async () => {
      const { user } = await openWorkspace();
      await user.keyboard("{Meta>}{Shift>}m{/Shift}{/Meta}");
      expect(useUi.getState().bigTerminal).toBe(true);
      expect(useUi.getState().dockOpen).toBe(true);
      await user.keyboard("{Meta>}{Shift>}m{/Shift}{/Meta}");
      expect(useUi.getState().bigTerminal).toBe(false);
      expect(useUi.getState().dockOpen).toBe(true);
    });

    it("has no terminal shortcuts away from a workspace", async () => {
      defaultBackend();
      const { user } = renderApp("/");
      await screen.findByRole("main");
      await user.keyboard("{Meta>}r{/Meta}");
      await user.keyboard("{Meta>}t{/Meta}");
      expect(backend.callsTo("script_run")).toHaveLength(0);
      expect(backend.callsTo("terminal_create")).toHaveLength(0);
    });

    it("lists the terminal shortcuts in the shortcuts dialog", async () => {
      const { user } = await openWorkspace();
      // user-event only knows the Slash key by its code.
      await user.keyboard("{Meta>}[Slash]{/Meta}");
      const dialog = await screen.findByRole("dialog", { name: "Keyboard shortcuts" });
      const terminalGroup = within(dialog).getByRole("heading", { name: "Terminal" })
        .parentElement as HTMLElement;
      expect(within(terminalGroup).getByText("Run")).toBeInTheDocument();
      expect(within(terminalGroup).getByText("⌘R")).toBeInTheDocument();
      expect(within(terminalGroup).getByText("⌘⇧R")).toBeInTheDocument();
      expect(within(terminalGroup).getByText("⌘T")).toBeInTheDocument();
      expect(within(terminalGroup).getByText("⌘J")).toBeInTheDocument();
    });
  });
});

describe("Quit confirmation", () => {
  it("asks before quitting while processes run, and quits on request", async () => {
    defaultBackend();
    backend.on("app_quit", null);
    const { user } = renderApp("/");
    await screen.findByRole("main");
    act(() => emitBackendEvent("app:confirm-quit", 2));

    const dialog = await screen.findByRole("dialog", { name: "Quit Circles?" });
    expect(dialog).toHaveTextContent("2 processes are still running");
    await user.click(within(dialog).getByRole("button", { name: "Quit and stop everything" }));
    await waitFor(() => expect(backend.callsTo("app_quit")).toHaveLength(1));
  });

  it("can be dismissed to keep everything running", async () => {
    defaultBackend();
    const { user } = renderApp("/");
    await screen.findByRole("main");
    act(() => emitBackendEvent("app:confirm-quit", 1));
    const dialog = await screen.findByRole("dialog", { name: "Quit Circles?" });
    expect(dialog).toHaveTextContent("1 process is still running");
    await user.click(within(dialog).getByRole("button", { name: "Keep running" }));
    await waitFor(() =>
      expect(screen.queryByRole("dialog", { name: "Quit Circles?" })).not.toBeInTheDocument(),
    );
    expect(backend.callsTo("app_quit")).toHaveLength(0);
  });
});
