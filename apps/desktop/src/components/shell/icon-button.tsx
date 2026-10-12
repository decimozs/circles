import { Button } from "@circles/ui/components/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@circles/ui/components/tooltip";
import { cn } from "@circles/ui/utils";
import type * as React from "react";

type IconButtonProps = Omit<React.ComponentProps<typeof Button>, "children"> & {
  label: string;
  hint?: string;
  children: React.ReactNode;
};

export function IconButton({ label, hint, className, children, ...props }: IconButtonProps) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={label}
            className={cn("text-muted-foreground hover:text-foreground", className)}
            {...props}
          />
        }
      >
        {children}
      </TooltipTrigger>
      <TooltipContent>
        {label}
        {hint ? <span className="ml-2 text-muted-foreground">{hint}</span> : null}
      </TooltipContent>
    </Tooltip>
  );
}
