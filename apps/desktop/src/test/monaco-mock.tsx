/**
 * Stand-in for the Monaco editors in jsdom, which has no layout or workers. It shows what the
 * editor was given and lets a test click a line in the gutter or type into the text.
 */
import type { CommentMark } from "@/features/diff/monaco-editors";

type CodeProps = {
  path: string;
  value: string;
  language: string;
  readOnly: boolean;
  marks?: CommentMark[];
  onChange?: (value: string) => void;
  onSave?: () => void;
  onComment?: (start: number, end: number) => void;
};

export function CodeView({
  path,
  value,
  language,
  readOnly,
  marks = [],
  onChange,
  onSave,
  onComment,
}: CodeProps) {
  return (
    <div
      data-testid="code-editor"
      data-path={path}
      data-language={language}
      data-readonly={String(readOnly)}
    >
      <pre>{value}</pre>
      <p data-testid="marks">
        {marks
          .map((m) => `${m.line_start}-${m.line_end}:${m.open ? "open" : "resolved"}`)
          .join(",")}
      </p>
      <textarea
        aria-label="Editor text"
        value={value}
        readOnly={readOnly}
        onChange={(e) => onChange?.(e.target.value)}
      />
      <button type="button" onClick={() => onComment?.(3, 3)}>
        Gutter line 3
      </button>
      <button type="button" onClick={() => onComment?.(5, 7)}>
        Gutter lines 5 to 7
      </button>
      <button type="button" onClick={() => onSave?.()}>
        Editor save
      </button>
    </div>
  );
}

type DiffProps = {
  path: string;
  original: string;
  modified: string;
  language: string;
  sideBySide: boolean;
  marks?: CommentMark[];
  onComment?: (start: number, end: number) => void;
};

export function DiffEditorView({
  path,
  original,
  modified,
  language,
  sideBySide,
  marks = [],
  onComment,
}: DiffProps) {
  return (
    <div
      data-testid="diff-editor"
      data-path={path}
      data-language={language}
      data-split={String(sideBySide)}
    >
      <pre data-testid="original">{original}</pre>
      <pre data-testid="modified">{modified}</pre>
      <p data-testid="marks">
        {marks
          .map((m) => `${m.line_start}-${m.line_end}:${m.open ? "open" : "resolved"}`)
          .join(",")}
      </p>
      <button type="button" onClick={() => onComment?.(3, 3)}>
        Gutter line 3
      </button>
      <button type="button" onClick={() => onComment?.(5, 7)}>
        Gutter lines 5 to 7
      </button>
    </div>
  );
}
