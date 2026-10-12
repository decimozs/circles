import { Popover, PopoverContent, PopoverTrigger } from "@circles/ui/components/popover";
import { Tooltip, TooltipContent, TooltipTrigger } from "@circles/ui/components/tooltip";
import { cn } from "@circles/ui/utils";
import { ChevronDownIcon, CircleHelpIcon } from "lucide-react";
import { useState } from "react";
import { DEFAULT_EFFORT, EFFORTS, effortLabel } from "@/lib/models";

type Props = {
  value: string | null;
  onChange: (effort: string) => void;
};

/** How hard Claude thinks, from faster to smarter, picked on a slider. */
export function EffortPicker({ value, onChange }: Props) {
  const [open, setOpen] = useState(false);
  const last = EFFORTS.length - 1;
  const found = EFFORTS.findIndex((e) => e.id === value);
  const index = found >= 0 ? found : EFFORTS.findIndex((e) => e.id === DEFAULT_EFFORT);
  const recommended = EFFORTS.findIndex((e) => e.id === DEFAULT_EFFORT);
  const percent = (i: number) => (i / last) * 100;

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger
        aria-label="Effort"
        className="flex h-7 items-center gap-1 rounded-md px-1.5 text-xs text-muted-foreground hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      >
        {value ? effortLabel(value) : "Effort"}
        <ChevronDownIcon className="size-3" />
      </PopoverTrigger>
      <PopoverContent side="top" align="end" className="w-64 gap-3 p-3">
        <div className="flex items-center justify-between">
          <p className="text-sm">
            <span className="text-muted-foreground">Effort</span>{" "}
            <span className="font-medium">{EFFORTS[index].label}</span>
          </p>
          <Tooltip>
            <TooltipTrigger
              aria-label="About effort"
              className="text-muted-foreground hover:text-foreground"
            >
              <CircleHelpIcon className="size-4" />
            </TooltipTrigger>
            <TooltipContent>
              Higher effort lets the agent think longer. Answers are better and slower.
            </TooltipContent>
          </Tooltip>
        </div>

        <div className="flex justify-between text-xs text-muted-foreground">
          <span>Faster</span>
          <span>Smarter</span>
        </div>

        <div className="relative mx-1 h-6">
          <div className="absolute inset-x-0 top-1/2 h-4 -translate-y-1/2 rounded-full bg-muted" />
          {EFFORTS.map((e, i) => (
            <span
              key={e.id}
              aria-hidden
              className={cn(
                "absolute top-1/2 size-1 -translate-x-1/2 -translate-y-1/2 rounded-full bg-muted-foreground/50",
                i === index && "opacity-0",
              )}
              style={{ left: `${percent(i)}%` }}
            />
          ))}
          <span
            aria-hidden
            className="absolute top-1/2 h-6 w-3 -translate-x-1/2 -translate-y-1/2 rounded-full bg-foreground shadow"
            style={{ left: `${percent(index)}%` }}
          />
          <input
            type="range"
            aria-label="Effort level"
            aria-valuetext={EFFORTS[index].label}
            min={0}
            max={last}
            step={1}
            value={index}
            onChange={(e) => onChange(EFFORTS[Number(e.target.value)].id)}
            className="absolute -inset-x-1 inset-y-0 w-[calc(100%+0.5rem)] cursor-pointer opacity-0"
          />
        </div>

        <div className="relative mx-1 h-4 text-xs text-muted-foreground">
          <span
            className={cn(
              "absolute whitespace-nowrap",
              recommended === 0 && "left-0",
              recommended === last && "right-0",
            )}
            style={
              recommended === 0 || recommended === last
                ? undefined
                : { left: `${percent(recommended)}%`, transform: "translateX(-50%)" }
            }
          >
            Recommended
          </span>
        </div>
      </PopoverContent>
    </Popover>
  );
}
