import { cn } from "@circles/ui/utils";
import type * as Monaco from "monaco-editor";
import { useEffect, useRef } from "react";
import { loadMonaco } from "./monaco";

const BASE_OPTIONS: Monaco.editor.IStandaloneEditorConstructionOptions = {
  automaticLayout: true,
  minimap: { enabled: false },
  scrollBeyondLastLine: false,
  glyphMargin: true,
  fontSize: 12,
  fontFamily: "JetBrains Mono Variable, ui-monospace, monospace",
  renderLineHighlight: "line",
  overviewRulerBorder: false,
  padding: { top: 8, bottom: 8 },
};

/** A line range of a comment, drawn in the gutter of the editor. */
export type CommentMark = { line_start: number; line_end: number; open: boolean };

function markDecorations(
  monaco: typeof Monaco,
  marks: CommentMark[],
): Monaco.editor.IModelDeltaDecoration[] {
  return marks.map((mark) => ({
    range: new monaco.Range(mark.line_start, 1, mark.line_end, 1),
    options: {
      isWholeLine: true,
      glyphMarginClassName: mark.open ? "comment-glyph" : "comment-glyph-resolved",
      className: mark.open ? "comment-line" : undefined,
    },
  }));
}

/** The line, or lines, a click in the gutter means: the selection when it spans several. */
function clickedRange(editor: Monaco.editor.ICodeEditor, line: number): [number, number] {
  const selection = editor.getSelection();
  if (
    selection &&
    !selection.isEmpty() &&
    line >= selection.startLineNumber &&
    line <= selection.endLineNumber
  ) {
    return [selection.startLineNumber, selection.endLineNumber];
  }
  return [line, line];
}

function onGutterClick(
  monaco: typeof Monaco,
  editor: Monaco.editor.ICodeEditor,
  handler: (start: number, end: number) => void,
) {
  return editor.onMouseDown((event) => {
    const type = event.target.type;
    const line = event.target.position?.lineNumber;
    if (
      line &&
      (type === monaco.editor.MouseTargetType.GUTTER_GLYPH_MARGIN ||
        type === monaco.editor.MouseTargetType.GUTTER_LINE_NUMBERS)
    ) {
      const [start, end] = clickedRange(editor, line);
      handler(start, end);
    }
  });
}

type CodeProps = {
  path: string;
  value: string;
  language: string;
  readOnly: boolean;
  marks?: CommentMark[];
  onChange?: (value: string) => void;
  onSave?: () => void;
  onComment?: (start: number, end: number) => void;
  className?: string;
};

/** One file in Monaco. */
export function CodeView({
  path,
  value,
  language,
  readOnly,
  marks = [],
  onChange,
  onSave,
  onComment,
  className,
}: CodeProps) {
  const host = useRef<HTMLDivElement>(null);
  const editor = useRef<Monaco.editor.IStandaloneCodeEditor | null>(null);
  const api = useRef<typeof Monaco | null>(null);
  const decorations = useRef<Monaco.editor.IEditorDecorationsCollection | null>(null);
  const handlers = useRef({ onChange, onSave, onComment });
  handlers.current = { onChange, onSave, onComment };

  // biome-ignore lint/correctness/useExhaustiveDependencies: the editor is made once per file
  useEffect(() => {
    let disposed = false;
    const cleanups: Array<() => void> = [];
    loadMonaco().then((monaco) => {
      if (disposed || !host.current) return;
      api.current = monaco;
      const model = monaco.editor.createModel(value, language, monaco.Uri.parse(`file:///${path}`));
      const instance = monaco.editor.create(host.current, { ...BASE_OPTIONS, model, readOnly });
      editor.current = instance;
      decorations.current = instance.createDecorationsCollection(markDecorations(monaco, marks));
      const changes = model.onDidChangeContent(() => handlers.current.onChange?.(model.getValue()));
      const clicks = onGutterClick(monaco, instance, (start, end) =>
        handlers.current.onComment?.(start, end),
      );
      cleanups.push(
        () => instance.dispose(),
        () => model.dispose(),
        () => changes.dispose(),
        () => clicks.dispose(),
      );
      instance.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () =>
        handlers.current.onSave?.(),
      );
    });
    return () => {
      disposed = true;
      for (const cleanup of cleanups) cleanup();
      editor.current = null;
    };
  }, [path, language]);

  useEffect(() => {
    editor.current?.updateOptions({ readOnly });
  }, [readOnly]);

  // The text changes when the file is reloaded from disk and the user has no edits of their own.
  useEffect(() => {
    const model = editor.current?.getModel();
    if (model && model.getValue() !== value) model.setValue(value);
  }, [value]);

  useEffect(() => {
    if (api.current) decorations.current?.set(markDecorations(api.current, marks));
  }, [marks]);

  return <div ref={host} data-testid="code-editor" className={cn("h-full min-h-0", className)} />;
}

type DiffProps = {
  path: string;
  original: string;
  modified: string;
  language: string;
  sideBySide: boolean;
  marks?: CommentMark[];
  onComment?: (start: number, end: number) => void;
  className?: string;
};

/** The two sides of a changed file in Monaco's diff editor. */
export function DiffEditorView({
  path,
  original,
  modified,
  language,
  sideBySide,
  marks = [],
  onComment,
  className,
}: DiffProps) {
  const host = useRef<HTMLDivElement>(null);
  const diff = useRef<Monaco.editor.IStandaloneDiffEditor | null>(null);
  const api = useRef<typeof Monaco | null>(null);
  const decorations = useRef<Monaco.editor.IEditorDecorationsCollection | null>(null);
  const onCommentRef = useRef(onComment);
  onCommentRef.current = onComment;

  // biome-ignore lint/correctness/useExhaustiveDependencies: the editor is rebuilt per file and content
  useEffect(() => {
    let disposed = false;
    const cleanups: Array<() => void> = [];
    loadMonaco().then((monaco) => {
      if (disposed || !host.current) return;
      api.current = monaco;
      const uri = (side: string) => monaco.Uri.parse(`diff-${side}:///${path}`);
      const originalModel = monaco.editor.createModel(original, language, uri("original"));
      const modifiedModel = monaco.editor.createModel(modified, language, uri("modified"));
      const instance = monaco.editor.createDiffEditor(host.current, {
        ...BASE_OPTIONS,
        readOnly: true,
        originalEditable: false,
        renderSideBySide: sideBySide,
        renderOverviewRuler: false,
        ignoreTrimWhitespace: false,
      });
      instance.setModel({ original: originalModel, modified: modifiedModel });
      diff.current = instance;
      const modifiedEditor = instance.getModifiedEditor();
      decorations.current = modifiedEditor.createDecorationsCollection(
        markDecorations(monaco, marks),
      );
      const clicks = onGutterClick(monaco, modifiedEditor, (start, end) =>
        onCommentRef.current?.(start, end),
      );
      cleanups.push(
        () => instance.dispose(),
        () => originalModel.dispose(),
        () => modifiedModel.dispose(),
        () => clicks.dispose(),
      );
    });
    return () => {
      disposed = true;
      for (const cleanup of cleanups) cleanup();
      diff.current = null;
    };
  }, [path, original, modified, language]);

  useEffect(() => {
    diff.current?.updateOptions({ renderSideBySide: sideBySide });
  }, [sideBySide]);

  useEffect(() => {
    if (api.current) decorations.current?.set(markDecorations(api.current, marks));
  }, [marks]);

  return <div ref={host} data-testid="diff-editor" className={cn("h-full min-h-0", className)} />;
}
