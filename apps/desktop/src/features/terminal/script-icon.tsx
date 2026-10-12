import {
  BugIcon,
  DatabaseIcon,
  FlaskConicalIcon,
  GlobeIcon,
  HammerIcon,
  type LucideIcon,
  PackageIcon,
  PlayIcon,
  RocketIcon,
  ServerIcon,
  SquareTerminalIcon,
} from "lucide-react";

/** The few lucide icons a run script can ask for by name. Anything else shows a play icon. */
const ICONS: Record<string, LucideIcon> = {
  play: PlayIcon,
  server: ServerIcon,
  "flask-conical": FlaskConicalIcon,
  globe: GlobeIcon,
  terminal: SquareTerminalIcon,
  database: DatabaseIcon,
  hammer: HammerIcon,
  rocket: RocketIcon,
  bug: BugIcon,
  package: PackageIcon,
};

export function ScriptIcon({ name, className }: { name: string | null; className?: string }) {
  const Icon = (name && ICONS[name]) || PlayIcon;
  return <Icon className={className} />;
}
