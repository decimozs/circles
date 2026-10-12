import type { Todo } from "@circles/bindings/Todo";
import { Input } from "@circles/ui/components/input";
import { cn } from "@circles/ui/utils";
import { PlusIcon, Trash2Icon } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import {
  useAddTodo,
  useCompleteTodo,
  useEditTodo,
  useRemoveTodo,
  useTodos,
} from "@/features/pr/api";

const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

function TodoRow({ workspaceId, todo }: { workspaceId: string; todo: Todo }) {
  const complete = useCompleteTodo(workspaceId);
  const edit = useEditTodo(workspaceId);
  const remove = useRemoveTodo(workspaceId);
  const [editing, setEditing] = useState(false);
  return (
    <li className="group flex items-start gap-2 rounded-md px-1 py-1 text-xs hover:bg-muted/60">
      <input
        type="checkbox"
        aria-label={`Mark "${todo.text}" as done`}
        checked={todo.done}
        onChange={(e) => complete.mutate({ id: todo.id, done: e.target.checked })}
        className="mt-0.5 size-3.5 shrink-0 accent-foreground"
      />
      {editing ? (
        <input
          // biome-ignore lint/a11y/noAutofocus: focus moves to the field the user just opened
          autoFocus
          aria-label="Edit todo"
          defaultValue={todo.text}
          maxLength={500}
          className="min-w-0 flex-1 bg-transparent outline-none"
          onBlur={(e) => {
            const text = e.target.value.trim();
            setEditing(false);
            if (text && text !== todo.text) {
              edit.mutate({ id: todo.id, text }, { onError: (err) => toast.error(message(err)) });
            }
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") e.currentTarget.blur();
            if (e.key === "Escape") setEditing(false);
          }}
        />
      ) : (
        <button
          type="button"
          onClick={() => setEditing(true)}
          className={cn(
            "min-w-0 flex-1 text-left break-words",
            todo.done && "text-muted-foreground line-through",
          )}
        >
          {todo.text}
          {todo.source === "agent" ? (
            <span className="ml-1.5 rounded bg-muted px-1 py-px text-[10px] text-muted-foreground no-underline">
              agent
            </span>
          ) : null}
        </button>
      )}
      <button
        type="button"
        aria-label={`Delete "${todo.text}"`}
        onClick={() => remove.mutate(todo.id)}
        className="shrink-0 rounded p-0.5 text-muted-foreground opacity-0 group-hover:opacity-100 hover:text-destructive focus-visible:opacity-100"
      >
        <Trash2Icon className="size-3.5" />
      </button>
    </li>
  );
}

/** Things left to do. Open ones hold back the merge. */
export function TodosSection({ workspaceId }: { workspaceId: string }) {
  const todos = useTodos(workspaceId).data ?? [];
  const add = useAddTodo(workspaceId);
  const [text, setText] = useState("");
  const open = todos.filter((t) => !t.done).length;

  return (
    <section aria-label="Todos" className="flex flex-col gap-1.5">
      <h3 className="flex items-center justify-between text-xs font-medium">
        <span>Todos</span>
        <span className="font-normal text-muted-foreground">
          {open === 0 ? "none open" : `${open} open`}
        </span>
      </h3>
      <form
        className="flex items-center gap-1"
        onSubmit={(e) => {
          e.preventDefault();
          const value = text.trim();
          if (!value) return;
          add.mutate(
            { text: value },
            { onSuccess: () => setText(""), onError: (err) => toast.error(message(err)) },
          );
        }}
      >
        <Input
          aria-label="New todo"
          placeholder="Add a todo"
          value={text}
          maxLength={500}
          onChange={(e) => setText(e.target.value)}
          className="h-7 text-xs"
        />
        <button
          type="submit"
          aria-label="Add todo"
          disabled={!text.trim() || add.isPending}
          className="flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground disabled:opacity-40"
        >
          <PlusIcon className="size-4" />
        </button>
      </form>
      {todos.length > 0 ? (
        <ul className="flex flex-col">
          {todos.map((todo) => (
            <TodoRow key={todo.id} workspaceId={workspaceId} todo={todo} />
          ))}
        </ul>
      ) : null}
    </section>
  );
}
