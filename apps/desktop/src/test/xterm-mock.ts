/** Fake xterm for jsdom, which cannot render a real terminal. */

type KeyHandler = (event: { type: string; metaKey: boolean; key: string }) => boolean;

export class FakeTerminal {
  static instances: FakeTerminal[] = [];

  cols = 100;
  rows = 28;
  options: Record<string, unknown>;
  written: (string | Uint8Array)[] = [];
  resets = 0;
  disposed = false;
  focused = false;
  private dataHandlers: ((data: string) => void)[] = [];
  private resizeHandlers: ((size: { cols: number; rows: number }) => void)[] = [];
  private keyHandler: KeyHandler | null = null;

  constructor(options: Record<string, unknown> = {}) {
    this.options = options;
    FakeTerminal.instances.push(this);
  }

  static reset() {
    FakeTerminal.instances = [];
  }

  static get last(): FakeTerminal {
    const terminal = FakeTerminal.instances[FakeTerminal.instances.length - 1];
    if (!terminal) throw new Error("no terminal was created");
    return terminal;
  }

  loadAddon() {}
  open() {}
  focus() {
    this.focused = true;
  }
  write(data: string | Uint8Array) {
    this.written.push(data);
  }
  reset() {
    this.resets += 1;
    this.written = [];
  }
  dispose() {
    this.disposed = true;
  }
  attachCustomKeyEventHandler(handler: KeyHandler) {
    this.keyHandler = handler;
  }
  onData(handler: (data: string) => void) {
    this.dataHandlers.push(handler);
    return { dispose: () => {} };
  }
  onResize(handler: (size: { cols: number; rows: number }) => void) {
    this.resizeHandlers.push(handler);
    return { dispose: () => {} };
  }

  // Test helpers
  type(data: string) {
    for (const handler of this.dataHandlers) handler(data);
  }
  resize(cols: number, rows: number) {
    this.cols = cols;
    this.rows = rows;
    for (const handler of this.resizeHandlers) handler({ cols, rows });
  }
  press(init: { metaKey?: boolean; key: string }): boolean {
    return this.keyHandler?.({ type: "keydown", metaKey: false, ...init }) ?? true;
  }
  /** Everything written so far, as text. */
  get text(): string {
    return this.written
      .map((chunk) => (typeof chunk === "string" ? chunk : new TextDecoder().decode(chunk)))
      .join("");
  }
}

export class FakeFitAddon {
  fit() {}
}

export class FakeSearchAddon {
  static searches: string[] = [];
  findNext(query: string) {
    FakeSearchAddon.searches.push(`next:${query}`);
    return true;
  }
  findPrevious(query: string) {
    FakeSearchAddon.searches.push(`prev:${query}`);
    return true;
  }
}

export class FakeWebLinksAddon {}
