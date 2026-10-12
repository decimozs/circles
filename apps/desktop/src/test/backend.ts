/** A fake Tauri backend for component tests. Commands are answered by registered handlers. */

type Handler = (args: Record<string, unknown>) => unknown;

export type Call = { command: string; args: Record<string, unknown> };

export class FakeBackend {
  private handlers = new Map<string, Handler>();
  calls: Call[] = [];

  reset() {
    this.handlers.clear();
    this.calls = [];
  }

  /** Answers `command` with a value, or with whatever `answer` returns or throws. */
  on(command: string, answer: unknown | Handler) {
    this.handlers.set(command, typeof answer === "function" ? (answer as Handler) : () => answer);
    return this;
  }

  /** Makes `command` fail with the same shape the Rust side uses. */
  fail(command: string, error: { code: string; message: string; details?: unknown }) {
    this.handlers.set(command, () => {
      throw { details: null, ...error };
    });
    return this;
  }

  async invoke(command: string, args: Record<string, unknown> = {}): Promise<unknown> {
    this.calls.push({ command, args });
    const handler = this.handlers.get(command);
    if (!handler) {
      throw { code: "internal", message: `No fake registered for ${command}`, details: null };
    }
    return handler(args);
  }

  callsTo(command: string): Record<string, unknown>[] {
    return this.calls.filter((c) => c.command === command).map((c) => c.args);
  }
}

export const backend = new FakeBackend();

type Listener = (event: { payload: unknown }) => void;
const listeners = new Map<string, Set<Listener>>();

export function addListener(name: string, listener: Listener): () => void {
  const set = listeners.get(name) ?? new Set();
  set.add(listener);
  listeners.set(name, set);
  return () => set.delete(listener);
}

/** Delivers a backend event, such as `workspace:progress`, to the listeners. */
export function emitBackendEvent(name: string, payload: unknown) {
  for (const listener of listeners.get(name) ?? []) listener({ payload });
}

export function resetListeners() {
  listeners.clear();
}

/** Stand-in for Tauri's `Channel`: the test pushes backend messages through `emit`. */
export class FakeChannel {
  static instances: FakeChannel[] = [];
  onmessage: (message: unknown) => void = () => {};

  constructor() {
    FakeChannel.instances.push(this);
  }

  static reset() {
    FakeChannel.instances = [];
  }

  static get last(): FakeChannel {
    const channel = FakeChannel.instances[FakeChannel.instances.length - 1];
    if (!channel) throw new Error("no channel was created");
    return channel;
  }

  emit(message: unknown) {
    this.onmessage(message);
  }
}
