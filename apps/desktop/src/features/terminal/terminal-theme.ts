import type { ITheme } from "@xterm/xterm";

const ANSI = [
  "black",
  "red",
  "green",
  "yellow",
  "blue",
  "magenta",
  "cyan",
  "white",
  "bright-black",
  "bright-red",
  "bright-green",
  "bright-yellow",
  "bright-blue",
  "bright-magenta",
  "bright-cyan",
  "bright-white",
] as const;

const XTERM_KEYS = [
  "black",
  "red",
  "green",
  "yellow",
  "blue",
  "magenta",
  "cyan",
  "white",
  "brightBlack",
  "brightRed",
  "brightGreen",
  "brightYellow",
  "brightBlue",
  "brightMagenta",
  "brightCyan",
  "brightWhite",
] as const;

/** The terminal colors for the current light or dark theme, read from the design tokens. */
export function readTerminalTheme(): ITheme {
  const style = getComputedStyle(document.documentElement);
  const token = (name: string) => style.getPropertyValue(`--terminal-${name}`).trim() || undefined;

  const theme: ITheme = {
    background: token("background"),
    foreground: token("foreground"),
    cursor: token("cursor"),
    cursorAccent: token("background"),
    selectionBackground: token("selection"),
  };
  ANSI.forEach((name, i) => {
    theme[XTERM_KEYS[i]] = token(name);
  });
  return theme;
}
