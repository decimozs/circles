import { errorFix } from "@/lib/ipc";

/** Inline error with the fix command when the backend provides one. */
export function ErrorMessage({ error }: { error: unknown }) {
  if (!error) return null;
  const message = error instanceof Error ? error.message : String(error);
  const fix = errorFix(error);
  return (
    <div role="alert" className="flex flex-col gap-1.5 text-sm text-destructive">
      <p>{message}</p>
      {fix ? (
        <code className="w-fit rounded bg-muted px-1.5 py-0.5 font-mono text-xs text-foreground">
          {fix}
        </code>
      ) : null}
    </div>
  );
}
