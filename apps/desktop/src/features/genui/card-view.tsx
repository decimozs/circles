import type { Card } from "@circles/bindings/Card";
import type { Component } from "@circles/bindings/Component";
import type { Step } from "@circles/bindings/Step";
import type { StoredCard } from "@circles/bindings/StoredCard";
import { Badge } from "@circles/ui/components/badge";
import { Button } from "@circles/ui/components/button";
import { Input } from "@circles/ui/components/input";
import { Textarea } from "@circles/ui/components/textarea";
import { cn } from "@circles/ui/utils";
import { ArrowDownIcon, ArrowUpIcon, CheckIcon, XIcon } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { useAnswerCard, useDismissCard } from "./api";

type Answers = Record<string, unknown>;

type StepState = { id: string; text: string; done: boolean };

/** The value a component starts with. */
function initial(component: Component): unknown {
  switch (component.type) {
    case "choice_list":
      return [];
    case "form":
      return {};
    case "steps":
      return component.steps.map((s: Step) => ({ id: s.id, text: s.text, done: s.done }));
    case "confirm":
    case "actions":
      return null;
    default:
      return undefined;
  }
}

function describe(component: Component, value: unknown): string {
  if (component.type === "choice_list") {
    const chosen = (value as string[] | undefined) ?? [];
    return chosen.map((v) => component.options.find((o) => o.value === v)?.label ?? v).join(", ");
  }
  if (component.type === "confirm") {
    return component.answers.find((a) => a.value === value)?.label ?? String(value ?? "");
  }
  if (component.type === "actions") {
    return component.buttons.find((b) => b.value === value)?.label ?? String(value ?? "");
  }
  return "";
}

/** One component. `value` and `onChange` are for the ones that take an answer. */
function ComponentView({
  component,
  value,
  onChange,
  readOnly,
  onPress,
}: {
  component: Component;
  value: unknown;
  onChange: (value: unknown) => void;
  readOnly: boolean;
  /** A button of a card with nothing else to fill in answers at once. */
  onPress: (value: unknown) => void;
}) {
  switch (component.type) {
    case "text":
      return <p className="whitespace-pre-wrap break-words text-sm">{component.text}</p>;
    case "key_value":
      return (
        <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-sm">
          {component.rows.map((row, i) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: a fixed list that never reorders
            <div key={i} className="contents">
              <dt className="text-muted-foreground">{row.label}</dt>
              <dd className="break-words">{row.value}</dd>
            </div>
          ))}
        </dl>
      );
    case "list":
      return (
        <ul className="flex flex-col gap-1 text-sm">
          {component.items.map((item, i) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: a fixed list that never reorders
            <li key={i} className="flex flex-col">
              <span>{item.label}</span>
              {item.detail ? (
                <span className="text-xs text-muted-foreground">{item.detail}</span>
              ) : null}
            </li>
          ))}
        </ul>
      );
    case "table":
      return (
        <div className="overflow-x-auto">
          <table className="w-full text-left text-xs">
            <thead>
              <tr className="border-b border-border text-muted-foreground">
                {component.columns.map((c, i) => (
                  // biome-ignore lint/suspicious/noArrayIndexKey: a fixed list that never reorders
                  <th key={i} className="px-2 py-1 font-medium">
                    {c}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {component.rows.map((row, r) => (
                // biome-ignore lint/suspicious/noArrayIndexKey: a fixed list that never reorders
                <tr key={r} className="border-b border-border/50">
                  {row.map((cell, c) => (
                    // biome-ignore lint/suspicious/noArrayIndexKey: a fixed list that never reorders
                    <td key={c} className="px-2 py-1 break-words">
                      {cell}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      );
    case "code":
      return (
        <pre className="max-h-60 overflow-auto rounded-md bg-muted/60 p-2 font-mono text-xs whitespace-pre-wrap break-words">
          {component.text}
        </pre>
      );
    case "progress":
      return (
        <ul className="flex flex-col gap-1 text-sm">
          {component.items.map((item, i) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: a fixed list that never reorders
            <li key={i} className="flex items-center gap-2">
              <span
                aria-hidden
                className={cn(
                  "size-2 rounded-full",
                  item.state === "done" && "bg-success",
                  item.state === "running" && "animate-pulse bg-warning",
                  item.state === "failed" && "bg-destructive",
                  item.state === "pending" && "bg-muted-foreground/40",
                )}
              />
              <span>{item.label}</span>
              <span className="text-xs text-muted-foreground">{item.state}</span>
            </li>
          ))}
        </ul>
      );
    case "choice_list": {
      const chosen = (value as string[] | undefined) ?? [];
      const toggle = (v: string) => {
        if (component.multiple) {
          onChange(chosen.includes(v) ? chosen.filter((c) => c !== v) : [...chosen, v]);
        } else {
          onChange([v]);
        }
      };
      return (
        <fieldset className="flex flex-col gap-1" aria-label={component.id}>
          {component.options.map((option) => (
            <label
              key={option.value}
              className={cn(
                "flex cursor-pointer items-start gap-2 rounded-md border border-border px-2 py-1.5 text-sm has-[:checked]:border-ring",
                readOnly && "cursor-default opacity-80",
              )}
            >
              <input
                type={component.multiple ? "checkbox" : "radio"}
                name={component.id}
                className="mt-1"
                disabled={readOnly}
                checked={chosen.includes(option.value)}
                onChange={() => toggle(option.value)}
              />
              <span className="flex min-w-0 flex-col">
                <span>{option.label}</span>
                {option.detail ? (
                  <span className="text-xs text-muted-foreground">{option.detail}</span>
                ) : null}
              </span>
            </label>
          ))}
        </fieldset>
      );
    }
    case "form": {
      const values = (value as Record<string, unknown> | undefined) ?? {};
      const set = (name: string, v: unknown) => onChange({ ...values, [name]: v });
      return (
        <div className="flex flex-col gap-2">
          {component.fields.map((field) => (
            <div key={field.name} className="flex flex-col gap-1 text-sm">
              <span className="text-xs font-medium">
                {field.label}
                {field.required ? <span className="text-destructive"> *</span> : null}
              </span>
              {field.kind === "long_text" ? (
                <Textarea
                  aria-label={field.label}
                  rows={3}
                  disabled={readOnly}
                  placeholder={field.placeholder ?? undefined}
                  value={String(values[field.name] ?? "")}
                  onChange={(e) => set(field.name, e.target.value)}
                />
              ) : field.kind === "checkbox" ? (
                <input
                  type="checkbox"
                  aria-label={field.label}
                  className="size-4 self-start"
                  disabled={readOnly}
                  checked={values[field.name] === true}
                  onChange={(e) => set(field.name, e.target.checked)}
                />
              ) : field.kind === "select" ? (
                <select
                  aria-label={field.label}
                  disabled={readOnly}
                  value={String(values[field.name] ?? "")}
                  onChange={(e) => set(field.name, e.target.value)}
                  className="h-8 rounded-md border border-input bg-background px-2 text-xs"
                >
                  <option value="">Choose…</option>
                  {field.options.map((o) => (
                    <option key={o.value} value={o.value}>
                      {o.label}
                    </option>
                  ))}
                </select>
              ) : (
                <Input
                  aria-label={field.label}
                  type={field.kind === "number" ? "number" : "text"}
                  disabled={readOnly}
                  placeholder={field.placeholder ?? undefined}
                  value={String(values[field.name] ?? "")}
                  onChange={(e) =>
                    set(
                      field.name,
                      field.kind === "number" && e.target.value !== ""
                        ? Number(e.target.value)
                        : e.target.value,
                    )
                  }
                  className="h-8 text-sm"
                />
              )}
            </div>
          ))}
        </div>
      );
    }
    case "steps": {
      const steps = (value as StepState[] | undefined) ?? [];
      const move = (index: number, by: number) => {
        const next = [...steps];
        const [item] = next.splice(index, 1);
        next.splice(index + by, 0, item);
        onChange(next);
      };
      return (
        <ol aria-label={component.id} className="flex flex-col gap-1">
          {steps.map((step, index) => (
            <li
              key={step.id}
              className="flex items-center gap-2 rounded-md border border-border px-2 py-1 text-sm"
            >
              <input
                type="checkbox"
                aria-label={`Do: ${step.text}`}
                disabled={readOnly}
                checked={step.done}
                onChange={(e) =>
                  onChange(
                    steps.map((s) => (s.id === step.id ? { ...s, done: e.target.checked } : s)),
                  )
                }
              />
              <span
                className={cn("min-w-0 flex-1", !step.done && "text-muted-foreground line-through")}
              >
                {step.text}
              </span>
              {readOnly ? null : (
                <span className="flex shrink-0 items-center">
                  <button
                    type="button"
                    aria-label={`Move up: ${step.text}`}
                    disabled={index === 0}
                    onClick={() => move(index, -1)}
                    className="rounded p-1 text-muted-foreground hover:bg-muted disabled:opacity-30"
                  >
                    <ArrowUpIcon className="size-3.5" />
                  </button>
                  <button
                    type="button"
                    aria-label={`Move down: ${step.text}`}
                    disabled={index === steps.length - 1}
                    onClick={() => move(index, 1)}
                    className="rounded p-1 text-muted-foreground hover:bg-muted disabled:opacity-30"
                  >
                    <ArrowDownIcon className="size-3.5" />
                  </button>
                  <button
                    type="button"
                    aria-label={`Remove: ${step.text}`}
                    onClick={() => onChange(steps.filter((s) => s.id !== step.id))}
                    className="rounded p-1 text-muted-foreground hover:bg-muted"
                  >
                    <XIcon className="size-3.5" />
                  </button>
                </span>
              )}
            </li>
          ))}
        </ol>
      );
    }
    case "confirm":
      return (
        <fieldset className="flex flex-wrap gap-2" aria-label={component.id}>
          {component.answers.map((answer) => (
            <Button
              key={answer.value}
              size="sm"
              variant={value === answer.value ? "default" : "outline"}
              disabled={readOnly}
              aria-pressed={value === answer.value}
              onClick={() => {
                onChange(answer.value);
                onPress(answer.value);
              }}
            >
              {answer.label}
            </Button>
          ))}
        </fieldset>
      );
    case "actions":
      return (
        <fieldset className="flex flex-wrap gap-2" aria-label={component.id}>
          {component.buttons.map((button) => (
            <Button
              key={button.value}
              size="sm"
              variant={
                button.style === "danger"
                  ? "destructive"
                  : value === button.value || button.style === "primary"
                    ? "default"
                    : "outline"
              }
              disabled={readOnly}
              onClick={() => {
                onChange(button.value);
                onPress(button.value);
              }}
            >
              {button.label}
            </Button>
          ))}
        </fieldset>
      );
  }
}

/** Whether what is filled in is enough to send. The app checks it again before it counts. */
function complete(card: Card, answers: Answers): boolean {
  for (const component of card.body) {
    if (component.type === "choice_list" && component.required) {
      if (((answers[component.id] as string[] | undefined) ?? []).length === 0) return false;
    }
    if (component.type === "form") {
      const values = (answers[component.id] as Record<string, unknown> | undefined) ?? {};
      for (const field of component.fields) {
        const v = values[field.name];
        if (field.required && (v === undefined || v === "" || v === null)) return false;
      }
    }
    if (component.type === "confirm" && answers[component.id] == null) return false;
  }
  return true;
}

/**
 * A card an agent showed. Waiting, it can be filled in and sent, or dismissed. Answered, it shows
 * what was chosen and cannot be changed.
 */
export function CardView({ stored, from }: { stored: StoredCard; from?: string }) {
  const { card } = stored;
  const pending = stored.status === "pending";
  const answer = useAnswerCard();
  const dismiss = useDismissCard();
  const [answers, setAnswers] = useState<Answers>(() => {
    const given = (stored.answer as Answers | null) ?? null;
    const start: Answers = {};
    for (const component of card.body) {
      const id = "id" in component ? component.id : undefined;
      if (!id) continue;
      start[id] = given && id in given ? given[id] : initial(component);
    }
    return start;
  });

  const inputs = card.body.filter((c) => "id" in c);
  const soleButtons = inputs.length === 1 && ["actions", "confirm"].includes(inputs[0].type);

  const send = (override?: Answers) => {
    const values = override ?? answers;
    // Buttons that were not pressed are left out of the answer.
    const clean = Object.fromEntries(
      Object.entries(values).filter(([, v]) => v !== null && v !== undefined),
    );
    answer.mutate(
      { uid: stored.uid, values: clean },
      { onError: (e) => toast.error(e instanceof Error ? e.message : String(e)) },
    );
  };

  return (
    <section
      aria-label={card.title ?? "Question from the agent"}
      className={cn(
        "flex flex-col gap-3 rounded-lg border p-3 text-sm",
        pending ? "border-warning/60 bg-warning/5" : "border-border bg-card",
      )}
    >
      <header className="flex items-start gap-2">
        <div className="min-w-0 flex-1">
          {from ? <p className="text-xs text-muted-foreground">{from}</p> : null}
          {card.title ? <h3 className="font-medium">{card.title}</h3> : null}
        </div>
        {pending ? null : (
          <Badge variant="secondary" className="shrink-0 capitalize">
            {stored.status === "answered" ? (
              <>
                <CheckIcon className="size-3" />
                Answered
              </>
            ) : (
              stored.status
            )}
          </Badge>
        )}
      </header>

      {card.outside_text ? (
        <p className="text-xs text-muted-foreground">
          Includes text from outside, such as tickets or pull requests.
        </p>
      ) : null}

      <div className="flex flex-col gap-3">
        {card.body.map((component, index) => {
          const id = "id" in component ? component.id : undefined;
          // A finished card of one question can be read as its answer.
          return (
            <ComponentView
              key={id ?? index}
              component={component}
              value={id ? answers[id] : undefined}
              readOnly={!pending || answer.isPending}
              onChange={(value) => id && setAnswers((a) => ({ ...a, [id]: value }))}
              onPress={(value) => {
                if (soleButtons && id && pending) send({ [id]: value });
              }}
            />
          );
        })}
      </div>

      {pending ? (
        <footer className="flex items-center justify-end gap-2">
          <Button
            size="sm"
            variant="ghost"
            disabled={dismiss.isPending || answer.isPending}
            onClick={() => dismiss.mutate(stored.uid)}
          >
            Not now
          </Button>
          {soleButtons ? null : (
            <Button
              size="sm"
              disabled={answer.isPending || !complete(card, answers)}
              onClick={() => send()}
            >
              {card.submit_label ?? "Send"}
            </Button>
          )}
        </footer>
      ) : stored.status === "answered" ? (
        <footer className="text-xs text-muted-foreground">
          {card.body
            .filter((c) => "id" in c)
            .map((c) => describe(c, "id" in c ? answers[c.id] : undefined))
            .filter(Boolean)
            .join(" · ")}
        </footer>
      ) : null}
    </section>
  );
}
