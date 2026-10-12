import { Badge } from "@circles/ui/components/badge";
import { Button } from "@circles/ui/components/button";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandShortcut,
} from "@circles/ui/components/command";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from "@circles/ui/components/context-menu";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@circles/ui/components/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@circles/ui/components/dropdown-menu";
import { Input } from "@circles/ui/components/input";
import { Popover, PopoverContent, PopoverTrigger } from "@circles/ui/components/popover";
import { ScrollArea } from "@circles/ui/components/scroll-area";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@circles/ui/components/select";
import { Separator } from "@circles/ui/components/separator";
import { Switch } from "@circles/ui/components/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@circles/ui/components/tabs";
import { Textarea } from "@circles/ui/components/textarea";
import { Tooltip, TooltipContent, TooltipTrigger } from "@circles/ui/components/tooltip";
import { toast } from "sonner";
import { ThemeToggle } from "@/components/theme-toggle";

const SURFACES = [
  "background",
  "sidebar",
  "card",
  "popover",
  "muted",
  "secondary",
  "accent",
  "primary",
] as const;

const SCROLL_ROWS = [
  "Row 1",
  "Row 2",
  "Row 3",
  "Row 4",
  "Row 5",
  "Row 6",
  "Row 7",
  "Row 8",
] as const;

const STATUS = ["destructive", "success", "warning", "info"] as const;

const TERMINAL = [
  "black",
  "red",
  "green",
  "yellow",
  "blue",
  "magenta",
  "cyan",
  "white",
  "bright-black",
  "bright-red",
  "bright-green",
  "bright-yellow",
  "bright-blue",
  "bright-magenta",
  "bright-cyan",
  "bright-white",
] as const;

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="flex flex-col gap-3">
      <h2 className="text-xs font-medium uppercase tracking-wide text-muted-foreground">{title}</h2>
      <div className="flex flex-wrap items-start gap-3">{children}</div>
    </section>
  );
}

function Swatch({ name, color }: { name: string; color: string }) {
  return (
    <div className="flex w-28 flex-col gap-1.5">
      <div className="h-10 rounded-md border border-border" style={{ background: color }} />
      <span className="text-2xs text-muted-foreground">{name}</span>
    </div>
  );
}

export function DesignGallery() {
  return (
    <div className="h-full overflow-y-auto bg-background">
      <div className="mx-auto flex max-w-4xl flex-col gap-10 px-6 py-10">
        <header className="flex items-center justify-between">
          <div>
            <h1 className="text-xl font-semibold">Design gallery</h1>
            <p className="text-sm text-muted-foreground">Every token and base component.</p>
          </div>
          <div className="flex items-center gap-3">
            <a className="text-xs text-muted-foreground underline underline-offset-4" href="#/">
              Back
            </a>
            <ThemeToggle />
          </div>
        </header>

        <Section title="Surfaces">
          {SURFACES.map((n) => (
            <Swatch key={n} name={n} color={`var(--${n})`} />
          ))}
        </Section>

        <Section title="Status and diff">
          {STATUS.map((n) => (
            <Swatch key={n} name={n} color={`var(--${n})`} />
          ))}
          <Swatch name="diff-add" color="var(--diff-add)" />
          <Swatch name="diff-add-bg" color="var(--diff-add-bg)" />
          <Swatch name="diff-remove" color="var(--diff-remove)" />
          <Swatch name="diff-remove-bg" color="var(--diff-remove-bg)" />
        </Section>

        <Section title="Terminal">
          <div
            className="flex w-full flex-wrap gap-3 rounded-lg border border-border p-3 font-mono text-xs"
            style={{
              background: "var(--terminal-background)",
              color: "var(--terminal-foreground)",
            }}
          >
            {TERMINAL.map((n) => (
              <span key={n} style={{ color: `var(--terminal-${n})` }}>
                {n}
              </span>
            ))}
          </div>
        </Section>

        <Section title="Typography">
          <div className="flex flex-col gap-1">
            <p className="text-2xl font-semibold">Heading 2xl</p>
            <p className="text-xl font-semibold">Heading xl</p>
            <p className="text-lg">Large text</p>
            <p className="text-base">Base text 14px</p>
            <p className="text-sm">UI text 13px (default)</p>
            <p className="text-xs">Small text 12px</p>
            <p className="text-2xs">Tiny text 11px</p>
            <p className="font-mono text-sm">const circles = "JetBrains Mono";</p>
          </div>
        </Section>

        <Section title="Radii and shadows">
          {(["sm", "md", "lg", "xl"] as const).map((r) => (
            <div
              key={r}
              className={`flex size-20 items-center justify-center border border-border bg-card text-xs rounded-${r} shadow-md`}
            >
              {r}
            </div>
          ))}
        </Section>

        <Section title="Buttons">
          <Button>Default</Button>
          <Button variant="secondary">Secondary</Button>
          <Button variant="outline">Outline</Button>
          <Button variant="ghost">Ghost</Button>
          <Button variant="destructive">Destructive</Button>
          <Button variant="link">Link</Button>
          <Button disabled>Disabled</Button>
          <Button size="sm">Small</Button>
          <Button size="xs">Extra small</Button>
        </Section>

        <Section title="Badges">
          <Badge>Default</Badge>
          <Badge variant="secondary">Secondary</Badge>
          <Badge variant="outline">Outline</Badge>
          <Badge variant="destructive">Destructive</Badge>
        </Section>

        <Section title="Inputs">
          <Input className="w-64" placeholder="Search" />
          <Input className="w-64" placeholder="Disabled" disabled />
          <Textarea className="w-64" placeholder="What do you want to work on?" />
          <div className="flex items-center gap-2">
            <Switch defaultChecked />
            <Switch />
          </div>
          <Select defaultValue="opus">
            <SelectTrigger className="w-44">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="opus">Opus</SelectItem>
              <SelectItem value="sonnet">Sonnet</SelectItem>
              <SelectItem value="haiku">Haiku</SelectItem>
            </SelectContent>
          </Select>
        </Section>

        <Section title="Tabs">
          <Tabs defaultValue="files" className="w-80">
            <TabsList>
              <TabsTrigger value="files">All files</TabsTrigger>
              <TabsTrigger value="changes">Changes</TabsTrigger>
              <TabsTrigger value="checks">Checks</TabsTrigger>
            </TabsList>
            <TabsContent value="files" className="text-sm text-muted-foreground">
              File tree goes here.
            </TabsContent>
            <TabsContent value="changes" className="text-sm text-muted-foreground">
              Changes go here.
            </TabsContent>
            <TabsContent value="checks" className="text-sm text-muted-foreground">
              Checks go here.
            </TabsContent>
          </Tabs>
        </Section>

        <Section title="Overlays">
          <Dialog>
            <DialogTrigger render={<Button variant="outline" />}>Dialog</DialogTrigger>
            <DialogContent>
              <DialogHeader>
                <DialogTitle>Dialog title</DialogTitle>
                <DialogDescription>Dialog description text.</DialogDescription>
              </DialogHeader>
            </DialogContent>
          </Dialog>
          <Popover>
            <PopoverTrigger render={<Button variant="outline" />}>Popover</PopoverTrigger>
            <PopoverContent>Popover content</PopoverContent>
          </Popover>
          <Tooltip>
            <TooltipTrigger render={<Button variant="outline" />}>Tooltip</TooltipTrigger>
            <TooltipContent>Tooltip text</TooltipContent>
          </Tooltip>
          <DropdownMenu>
            <DropdownMenuTrigger render={<Button variant="outline" />}>
              Dropdown
            </DropdownMenuTrigger>
            <DropdownMenuContent>
              <DropdownMenuItem>Rename</DropdownMenuItem>
              <DropdownMenuItem>Archive</DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem>Delete</DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
          <ContextMenu>
            <ContextMenuTrigger className="flex h-9 items-center rounded-md border border-dashed border-border px-3 text-xs text-muted-foreground">
              Right-click me
            </ContextMenuTrigger>
            <ContextMenuContent>
              <ContextMenuItem>Copy path</ContextMenuItem>
              <ContextMenuItem>Reveal in Finder</ContextMenuItem>
            </ContextMenuContent>
          </ContextMenu>
          <Button
            variant="outline"
            onClick={() => toast.success("Workspace created", { description: "algiers" })}
          >
            Toast
          </Button>
        </Section>

        <Section title="Command">
          <Command className="w-96 rounded-lg border border-border">
            <CommandInput placeholder="Type a command or search..." />
            <CommandList>
              <CommandEmpty>No results.</CommandEmpty>
              <CommandGroup heading="Actions">
                <CommandItem>
                  Create workspace <CommandShortcut>⌘N</CommandShortcut>
                </CommandItem>
                <CommandItem>
                  Add project <CommandShortcut>⌘⌥A</CommandShortcut>
                </CommandItem>
                <CommandItem>
                  Toggle theme <CommandShortcut>⌘⌥T</CommandShortcut>
                </CommandItem>
              </CommandGroup>
            </CommandList>
          </Command>
        </Section>

        <Section title="Scroll area and separator">
          <ScrollArea className="h-24 w-64 rounded-md border border-border">
            <div className="flex flex-col gap-2 p-3 text-sm">
              {SCROLL_ROWS.map((row) => (
                <span key={row}>{row}</span>
              ))}
            </div>
          </ScrollArea>
          <Separator className="w-64" />
        </Section>
      </div>
    </div>
  );
}
