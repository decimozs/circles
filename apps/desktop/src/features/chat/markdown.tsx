import { cn } from "@circles/ui/utils";
import { openUrl } from "@tauri-apps/plugin-opener";
import { CheckIcon, CopyIcon } from "lucide-react";
import { type ComponentProps, isValidElement, memo, type ReactNode, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

export function useCopy() {
  const [copied, setCopied] = useState(false);
  const copy = (text: string) => {
    navigator.clipboard
      ?.writeText(text)
      .then(() => {
        setCopied(true);
        setTimeout(() => setCopied(false), 1500);
      })
      .catch(() => {});
  };
  return { copied, copy };
}

export function CopyButton({
  text,
  label = "Copy",
  className,
}: {
  text: string;
  label?: string;
  className?: string;
}) {
  const { copied, copy } = useCopy();
  return (
    <button
      type="button"
      aria-label={copied ? "Copied" : label}
      onClick={() => copy(text)}
      className={cn(
        "inline-flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground",
        className,
      )}
    >
      {copied ? <CheckIcon className="size-3.5" /> : <CopyIcon className="size-3.5" />}
    </button>
  );
}

function textOf(node: ReactNode): string {
  if (typeof node === "string") return node;
  if (Array.isArray(node)) return node.map(textOf).join("");
  if (isValidElement<{ children?: ReactNode }>(node)) return textOf(node.props.children);
  return "";
}

function Pre({ children }: ComponentProps<"pre">) {
  const code = textOf(children).replace(/\n$/, "");
  const language = /language-(\S+)/.exec(
    isValidElement<{ className?: string }>(children) ? (children.props.className ?? "") : "",
  )?.[1];
  return (
    <div className="group/code relative my-2 overflow-hidden rounded-md border border-border bg-muted/50">
      <div className="flex h-7 items-center justify-between border-b border-border px-2 text-[11px] text-muted-foreground">
        <span>{language ?? "text"}</span>
        <CopyButton text={code} label="Copy code" className="size-5" />
      </div>
      <pre className="overflow-x-auto p-3 font-mono text-xs leading-relaxed">{children}</pre>
    </div>
  );
}

const components: ComponentProps<typeof ReactMarkdown>["components"] = {
  pre: Pre,
  code: ({ className, children, ...rest }) =>
    className ? (
      <code className={className} {...rest}>
        {children}
      </code>
    ) : (
      <code className="rounded bg-muted px-1 py-0.5 font-mono text-[0.85em]" {...rest}>
        {children}
      </code>
    ),
  a: ({ href, children }) => (
    <a
      href={href}
      className="text-primary underline underline-offset-2"
      onClick={(event) => {
        event.preventDefault();
        if (href) openUrl(href).catch(() => {});
      }}
    >
      {children}
    </a>
  ),
  table: ({ children }) => (
    <div className="my-2 overflow-x-auto">
      <table className="w-full border-collapse text-xs">{children}</table>
    </div>
  ),
  th: ({ children }) => (
    <th className="border border-border bg-muted/50 px-2 py-1 text-left font-medium">{children}</th>
  ),
  td: ({ children }) => <td className="border border-border px-2 py-1">{children}</td>,
  ul: ({ children }) => <ul className="my-1.5 list-disc pl-5">{children}</ul>,
  ol: ({ children }) => <ol className="my-1.5 list-decimal pl-5">{children}</ol>,
  p: ({ children }) => <p className="my-1.5 first:mt-0 last:mb-0">{children}</p>,
  h1: ({ children }) => <h1 className="mt-3 mb-1.5 text-base font-semibold">{children}</h1>,
  h2: ({ children }) => <h2 className="mt-3 mb-1.5 text-sm font-semibold">{children}</h2>,
  h3: ({ children }) => <h3 className="mt-2 mb-1 text-sm font-medium">{children}</h3>,
  blockquote: ({ children }) => (
    <blockquote className="my-2 border-l-2 border-border pl-3 text-muted-foreground">
      {children}
    </blockquote>
  ),
  hr: () => <hr className="my-3 border-border" />,
};

/** Assistant text as markdown. Finished messages do not re-render while another one streams. */
export const Markdown = memo(function Markdown({ text }: { text: string }) {
  return (
    <div className="break-words text-sm leading-relaxed">
      <ReactMarkdown remarkPlugins={[remarkGfm]} components={components}>
        {text}
      </ReactMarkdown>
    </div>
  );
});
