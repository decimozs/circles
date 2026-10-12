import { cn } from "@circles/ui/utils";
import { MonitorIcon, MoonIcon, SunIcon } from "lucide-react";
import { type Theme, useTheme } from "@/components/theme-provider";

const OPTIONS: { value: Theme; label: string; icon: typeof SunIcon }[] = [
  { value: "light", label: "Light", icon: SunIcon },
  { value: "dark", label: "Dark", icon: MoonIcon },
  { value: "system", label: "System", icon: MonitorIcon },
];

export function ThemeToggle({ className }: { className?: string }) {
  const { theme, setTheme } = useTheme();

  return (
    <fieldset
      className={cn(
        "m-0 inline-flex min-w-0 items-center gap-0.5 rounded-lg border-0 bg-muted p-0.5",
        className,
      )}
    >
      <legend className="sr-only">Theme</legend>
      {OPTIONS.map(({ value, label, icon: Icon }) => {
        const active = theme === value;
        return (
          <button
            key={value}
            type="button"
            aria-pressed={active}
            aria-label={label}
            title={label}
            onClick={() => setTheme(value)}
            className={cn(
              "inline-flex size-7 items-center justify-center rounded-md text-muted-foreground outline-none transition-colors",
              "hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring/50",
              active && "bg-background text-foreground shadow-sm",
            )}
          >
            <Icon className="size-3.5" />
          </button>
        );
      })}
    </fieldset>
  );
}
