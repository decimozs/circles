import type * as Monaco from "monaco-editor";

type MonacoApi = typeof Monaco;

let loading: Promise<MonacoApi> | null = null;

let pixel: CanvasRenderingContext2D | null | undefined;

/**
 * A token as `#rrggbb`, or `#rrggbbaa` when it is see-through. Monaco only reads hex, rgb and hsl,
 * but a token can be a `color-mix(...)` (the Translucency setting makes the page colour one), which
 * the page reports as written. The browser works the colour out, and a canvas turns it into numbers.
 */
function cssColor(name: string): string {
  const root = document.documentElement;
  const raw = getComputedStyle(root).getPropertyValue(name).trim();
  if (/^#[0-9a-f]{6}$/i.test(raw)) return raw;
  if (pixel === undefined) {
    pixel = document.createElement("canvas").getContext("2d", { willReadFrequently: true });
  }
  if (!pixel) return raw;
  const probe = document.createElement("span");
  probe.style.color = `var(${name})`;
  root.appendChild(probe);
  const computed = getComputedStyle(probe).color;
  probe.remove();
  pixel.clearRect(0, 0, 1, 1);
  pixel.fillStyle = computed;
  pixel.fillRect(0, 0, 1, 1);
  const [r, g, b, a] = pixel.getImageData(0, 0, 1, 1).data;
  const hex = (n: number) => n.toString(16).padStart(2, "0");
  return `#${hex(r)}${hex(g)}${hex(b)}${a === 255 ? "" : hex(a)}`;
}

/** `#rrggbb` with an opacity, for the diff backgrounds that must let the text show through. */
function withAlpha(color: string, alpha: string): string {
  return /^#[0-9a-f]{6}$/i.test(color) ? `${color}${alpha}` : color;
}

/** Monaco's colours, taken from the app's tokens so the editor follows light and dark. */
export function applyTheme(monaco: MonacoApi) {
  const dark = document.documentElement.classList.contains("dark");
  monaco.editor.defineTheme("circles", {
    base: dark ? "vs-dark" : "vs",
    inherit: true,
    rules: [],
    colors: {
      "editor.background": cssColor("--background"),
      "editor.foreground": cssColor("--foreground"),
      "editorLineNumber.foreground": cssColor("--muted-foreground"),
      "editorLineNumber.activeForeground": cssColor("--foreground"),
      "editor.lineHighlightBackground": withAlpha(cssColor("--muted"), "80"),
      "editor.selectionBackground": withAlpha(cssColor("--accent"), "cc"),
      "editorCursor.foreground": cssColor("--foreground"),
      "editorGutter.background": cssColor("--background"),
      "editorWidget.background": cssColor("--popover"),
      "editorWidget.border": cssColor("--border"),
      "scrollbarSlider.background": withAlpha(cssColor("--border"), "99"),
      "diffEditor.insertedLineBackground": cssColor("--diff-add-bg"),
      "diffEditor.removedLineBackground": cssColor("--diff-remove-bg"),
      "diffEditor.insertedTextBackground": withAlpha(cssColor("--diff-add"), "33"),
      "diffEditor.removedTextBackground": withAlpha(cssColor("--diff-remove"), "33"),
      "diffEditorGutter.insertedLineBackground": cssColor("--diff-add-bg"),
      "diffEditorGutter.removedLineBackground": cssColor("--diff-remove-bg"),
    },
  });
  monaco.editor.setTheme("circles");
}

/**
 * Loads Monaco the first time an editor is needed. It and its workers are bundled with the app,
 * never fetched from a CDN, and stay out of the first load.
 */
export function loadMonaco(): Promise<MonacoApi> {
  loading ??= (async () => {
    const [monaco, editor, json, css, html, ts] = await Promise.all([
      import("monaco-editor"),
      import("monaco-editor/editor/editor.worker.js?worker"),
      import("monaco-editor/language/json/json.worker.js?worker"),
      import("monaco-editor/language/css/css.worker.js?worker"),
      import("monaco-editor/language/html/html.worker.js?worker"),
      import("monaco-editor/language/typescript/ts.worker.js?worker"),
    ]);
    self.MonacoEnvironment = {
      getWorker(_id: string, label: string) {
        if (label === "json") return new json.default();
        if (label === "css" || label === "scss" || label === "less") return new css.default();
        if (label === "html" || label === "handlebars" || label === "razor") {
          return new html.default();
        }
        if (label === "typescript" || label === "javascript") return new ts.default();
        return new editor.default();
      },
    };
    applyTheme(monaco);
    // The theme setting flips a class, and Translucency an attribute, on the page, so follow both.
    new MutationObserver(() => applyTheme(monaco)).observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["class", "data-translucent"],
    });
    return monaco;
  })();
  return loading;
}
