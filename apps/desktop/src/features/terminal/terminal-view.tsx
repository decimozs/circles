import { Button } from "@circles/ui/components/button";
import { Input } from "@circles/ui/components/input";
import { Channel } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { Terminal } from "@xterm/xterm";
import { ChevronDownIcon, ChevronUpIcon, XIcon } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTheme } from "@/components/theme-provider";
import { readTerminalTheme } from "@/features/terminal/terminal-theme";
import { ipc } from "@/lib/ipc";

type Control = { type: "exit"; code: number | null; signaled: boolean } | { type: "reset" };

function isControl(message: unknown): message is Control {
  return (
    typeof message === "object" &&
    message !== null &&
    !ArrayBuffer.isView(message) &&
    "type" in message &&
    typeof (message as { type: unknown }).type === "string"
  );
}

function exitLine(control: Extract<Control, { type: "exit" }>): string {
  const text =
    control.code === 0
      ? "Process finished"
      : control.code !== null
        ? `Process exited with code ${control.code}`
        : "Process was stopped";
  return `\r\n\x1b[2m[${text}]\x1b[0m\r\n`;
}

/**
 * One terminal tab. Output arrives as raw bytes over a Tauri channel, and input goes back as
 * text. Leaving the view only disconnects it: the process keeps running, and coming back
 * replays what it printed.
 */
export function TerminalView({ terminalId }: { terminalId: string }) {
  const host = useRef<HTMLDivElement>(null);
  const term = useRef<Terminal | null>(null);
  const searchAddon = useRef<SearchAddon | null>(null);
  const { resolvedTheme } = useTheme();
  const [searching, setSearching] = useState(false);
  const [query, setQuery] = useState("");

  useEffect(() => {
    const element = host.current;
    if (!element) return;

    const terminal = new Terminal({
      fontFamily: '"JetBrains Mono Variable", ui-monospace, Menlo, monospace',
      fontSize: 12,
      cursorBlink: true,
      scrollback: 5000,
      allowProposedApi: true,
      theme: readTerminalTheme(),
    });
    const fit = new FitAddon();
    const search = new SearchAddon();
    terminal.loadAddon(fit);
    terminal.loadAddon(search);
    terminal.loadAddon(
      new WebLinksAddon((_event, uri) => {
        openUrl(uri).catch(() => {});
      }),
    );
    terminal.open(element);
    fit.fit();
    term.current = terminal;
    searchAddon.current = search;

    // ⌘F searches this terminal's output instead of the page.
    terminal.attachCustomKeyEventHandler((event) => {
      if (event.type === "keydown" && event.metaKey && event.key === "f") {
        setSearching(true);
        return false;
      }
      return true;
    });

    const channel = new Channel<unknown>();
    channel.onmessage = (message) => {
      if (isControl(message)) {
        if (message.type === "reset") terminal.reset();
        else terminal.write(exitLine(message));
      } else {
        terminal.write(new Uint8Array(message as ArrayBuffer));
      }
    };
    ipc.terminalAttach(terminalId, terminal.cols, terminal.rows, channel).catch((error: Error) => {
      terminal.write(`\r\n\x1b[31m${error.message}\x1b[0m\r\n`);
    });

    const input = terminal.onData((data) => {
      ipc.terminalWrite(terminalId, data).catch(() => {});
    });
    const resized = terminal.onResize(({ cols, rows }) => {
      ipc.terminalResize(terminalId, cols, rows).catch(() => {});
    });
    const observer = new ResizeObserver(() => fit.fit());
    observer.observe(element);
    terminal.focus();

    return () => {
      observer.disconnect();
      input.dispose();
      resized.dispose();
      channel.onmessage = () => {};
      ipc.terminalDetach(terminalId).catch(() => {});
      terminal.dispose();
      term.current = null;
      searchAddon.current = null;
    };
  }, [terminalId]);

  // Follow the light and dark theme.
  // biome-ignore lint/correctness/useExhaustiveDependencies: re-read the tokens when the theme changes
  useEffect(() => {
    if (term.current) term.current.options.theme = readTerminalTheme();
  }, [resolvedTheme]);

  const find = (backwards: boolean) => {
    if (!query) return;
    if (backwards) searchAddon.current?.findPrevious(query);
    else searchAddon.current?.findNext(query);
  };

  return (
    <div className="relative h-full w-full bg-(--terminal-background)">
      <div ref={host} className="h-full w-full px-3 py-2" data-testid="terminal-host" />
      {searching ? (
        <form
          className="absolute top-2 right-4 flex items-center gap-1 rounded-lg border border-border bg-popover p-1 shadow-md"
          onSubmit={(e) => {
            e.preventDefault();
            find(false);
          }}
          onKeyDown={(e) => {
            // Escape closes the search from anywhere inside it, not only from the text field.
            if (e.key === "Escape") {
              setSearching(false);
              term.current?.focus();
            }
          }}
        >
          <Input
            autoFocus
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              searchAddon.current?.findNext(e.target.value);
            }}
            placeholder="Find"
            aria-label="Find in terminal"
            className="h-7 w-40 text-xs"
          />
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            aria-label="Previous match"
            onClick={() => find(true)}
          >
            <ChevronUpIcon />
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            aria-label="Next match"
            onClick={() => find(false)}
          >
            <ChevronDownIcon />
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            aria-label="Close search"
            onClick={() => {
              setSearching(false);
              term.current?.focus();
            }}
          >
            <XIcon />
          </Button>
        </form>
      ) : null}
    </div>
  );
}
