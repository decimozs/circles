import { Input } from "@circles/ui/components/input";
import { cn } from "@circles/ui/utils";
import { XIcon } from "lucide-react";
import { type KeyboardEvent, useId, useRef, useState } from "react";
import { useProjects } from "@/features/projects/api";
import { ProjectAvatar } from "@/features/projects/project-avatar";

/**
 * Picks repositories by searching, the way GitHub does: type, choose from what matches, and the
 * ones picked stay above as small tags that can be removed. A long list never fills the screen.
 */
export function RepoPicker({
  value,
  onChange,
}: {
  value: string[];
  onChange: (ids: string[]) => void;
}) {
  const projects = useProjects().data ?? [];
  const [search, setSearch] = useState("");
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const listId = useId();

  const picked = value
    .map((id) => projects.find((p) => p.id === id))
    .filter((p): p is NonNullable<typeof p> => !!p);
  const matches = projects
    .filter(
      (p) => !value.includes(p.id) && p.name.toLowerCase().includes(search.trim().toLowerCase()),
    )
    .slice(0, 8);

  const add = (id: string) => {
    onChange([...value, id]);
    setSearch("");
    setActive(0);
    input.current?.focus();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      setOpen(true);
      if (matches.length > 0) {
        const step = event.key === "ArrowDown" ? 1 : matches.length - 1;
        setActive((i) => (i + step) % matches.length);
      }
    } else if (event.key === "Enter" && open && matches[active]) {
      event.preventDefault();
      add(matches[active].id);
    } else if (event.key === "Escape" && open) {
      // Closes the list, not the dialog around it.
      event.preventDefault();
      event.stopPropagation();
      setOpen(false);
    } else if (event.key === "Backspace" && search === "" && value.length > 0) {
      onChange(value.slice(0, -1));
    }
  };

  return (
    <div className="flex flex-col gap-2">
      {picked.length > 0 ? (
        <ul aria-label="Picked repositories" className="flex flex-wrap gap-1.5">
          {picked.map((p) => (
            <li
              key={p.id}
              className="flex h-6 items-center gap-1.5 rounded-full border border-border bg-muted/50 pr-1 pl-1 text-xs"
            >
              <ProjectAvatar project={p} className="size-4" />
              <span className="max-w-40 truncate">{p.name}</span>
              <button
                type="button"
                aria-label={`Remove ${p.name}`}
                onClick={() => onChange(value.filter((v) => v !== p.id))}
                className="flex size-4 items-center justify-center rounded-full text-muted-foreground hover:bg-muted hover:text-foreground"
              >
                <XIcon className="size-3" />
              </button>
            </li>
          ))}
        </ul>
      ) : null}
      <div className="relative">
        <Input
          ref={input}
          role="combobox"
          aria-label="Search repositories"
          aria-expanded={open}
          aria-controls={listId}
          aria-autocomplete="list"
          aria-activedescendant={open && matches[active] ? `${listId}-${active}` : undefined}
          value={search}
          autoComplete="off"
          placeholder="Search repositories to add"
          className="h-8 text-xs"
          onChange={(e) => {
            setSearch(e.target.value);
            setActive(0);
            setOpen(true);
          }}
          onFocus={() => setOpen(true)}
          onBlur={() => setOpen(false)}
          onKeyDown={onKeyDown}
        />
        {open ? (
          <div
            id={listId}
            role="listbox"
            aria-label="Repositories"
            className="absolute top-full right-0 left-0 z-50 mt-1 max-h-48 overflow-y-auto rounded-lg border border-border bg-popover p-1 shadow-md"
          >
            {matches.map((p, index) => (
              <div
                key={p.id}
                id={`${listId}-${index}`}
                role="option"
                tabIndex={-1}
                aria-selected={index === active}
                // Before the input loses focus, so the list does not close under the click.
                onMouseDown={(e) => {
                  e.preventDefault();
                  add(p.id);
                }}
                onMouseEnter={() => setActive(index)}
                className={cn(
                  "flex cursor-default items-center gap-2 rounded-md px-2 py-1.5 text-xs",
                  index === active && "bg-muted",
                )}
              >
                <ProjectAvatar project={p} className="size-5" />
                <span className="truncate">{p.name}</span>
              </div>
            ))}
            {matches.length === 0 ? (
              <p className="px-2 py-1.5 text-xs text-muted-foreground">
                {projects.length === 0
                  ? "There are no repositories yet."
                  : search.trim()
                    ? `No repository matches "${search.trim()}".`
                    : "Every repository is picked."}
              </p>
            ) : null}
          </div>
        ) : null}
      </div>
    </div>
  );
}
