// Shortcuts match on `event.code` so macOS Option combos (which change `event.key`) still work.

export type Shortcut = {
  /** `KeyboardEvent.code`, e.g. "KeyB", "Period", "BracketLeft". */
  code: string;
  /** Command on macOS. */
  mod?: boolean;
  alt?: boolean;
  shift?: boolean;
};

const CODE_LABELS: Record<string, string> = {
  Period: ".",
  Comma: ",",
  Slash: "/",
  BracketLeft: "[",
  BracketRight: "]",
  Enter: "↵",
  Escape: "Esc",
  Backspace: "⌫",
  Space: "Space",
};

function codeLabel(code: string): string {
  if (code in CODE_LABELS) return CODE_LABELS[code];
  if (code.startsWith("Key")) return code.slice(3);
  if (code.startsWith("Digit")) return code.slice(5);
  return code;
}

export function formatShortcut(s: Shortcut): string {
  // Command first, to read like the shortcuts in the reference app: ⌘⌥B, not ⌥⌘B.
  return `${s.mod ? "⌘" : ""}${s.alt ? "⌥" : ""}${s.shift ? "⇧" : ""}${codeLabel(s.code)}`;
}

export function matchesShortcut(e: KeyboardEvent, s: Shortcut): boolean {
  return (
    e.code === s.code &&
    e.metaKey === !!s.mod &&
    e.altKey === !!s.alt &&
    e.shiftKey === !!s.shift &&
    !e.ctrlKey
  );
}

export function hasModifier(s: Shortcut): boolean {
  return !!(s.mod || s.alt);
}

export function shortcutKey(s: Shortcut): string {
  return `${s.mod ? "m" : ""}${s.alt ? "a" : ""}${s.shift ? "s" : ""}+${s.code}`;
}

export function isTypingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return target.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName);
}
