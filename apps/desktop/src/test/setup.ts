import "@testing-library/jest-dom/vitest";
import { open as pickFiles } from "@tauri-apps/plugin-dialog";
import { cleanup, configure } from "@testing-library/react";
import { afterEach, beforeEach, vi } from "vitest";
import { addListener, backend, FakeChannel, resetListeners } from "@/test/backend";
import { FakeTerminal } from "@/test/xterm-mock";

// The whole suite runs in parallel, so a screen can take longer than the default second to settle.
configure({ asyncUtilTimeout: 4000 });

// jsdom does not implement these browser APIs, which Base UI, cmdk and the panel library use.
class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver = ResizeObserverStub as unknown as typeof ResizeObserver;

Element.prototype.scrollIntoView = () => {};
Element.prototype.getAnimations = () => [];
Element.prototype.hasPointerCapture = () => false;
Element.prototype.setPointerCapture = () => {};
Element.prototype.releasePointerCapture = () => {};

window.matchMedia = ((query: string) => ({
  matches: false,
  media: query,
  onchange: null,
  addEventListener: () => {},
  removeEventListener: () => {},
  addListener: () => {},
  removeListener: () => {},
  dispatchEvent: () => false,
})) as typeof window.matchMedia;

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: Record<string, unknown>) => backend.invoke(command, args),
  Channel: FakeChannel,
}));

vi.mock("@xterm/xterm", async () => ({
  Terminal: (await import("@/test/xterm-mock")).FakeTerminal,
}));
vi.mock("@xterm/addon-fit", async () => ({
  FitAddon: (await import("@/test/xterm-mock")).FakeFitAddon,
}));
vi.mock("@xterm/addon-search", async () => ({
  SearchAddon: (await import("@/test/xterm-mock")).FakeSearchAddon,
}));
vi.mock("@xterm/addon-web-links", async () => ({
  WebLinksAddon: (await import("@/test/xterm-mock")).FakeWebLinksAddon,
}));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn(async () => {}) }));

vi.mock("@tauri-apps/api/event", () => ({
  listen: async (name: string, handler: (event: { payload: unknown }) => void) =>
    addListener(name, handler),
}));

vi.mock("react-resizable-panels", () => import("@/test/panels-mock"));
vi.mock("@/features/diff/monaco-editors", () => import("@/test/monaco-mock"));

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null) }));

beforeEach(() => {
  vi.mocked(pickFiles).mockResolvedValue(null);
  backend.reset();
  resetListeners();
  FakeChannel.reset();
  FakeTerminal.reset();
  localStorage.clear();
});

afterEach(() => {
  cleanup();
});
