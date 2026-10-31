import { useId, useState } from "react";
import { dayLabel, niceCeiling } from "./format";

export type ChartPoint = { day: string; value: number };

const W = 640;
const H = 220;
const PAD = { left: 52, right: 8, top: 10, bottom: 22 };

/** An area chart with one point per day. Hovering or focusing it shows a day's number. */
export function DailyChart({
  title,
  points,
  format,
}: {
  title: string;
  points: ChartPoint[];
  format: (value: number) => string;
}) {
  const gradient = useId();
  const [hover, setHover] = useState<number | null>(null);
  const top = niceCeiling(Math.max(0, ...points.map((p) => p.value)));
  const inner = { w: W - PAD.left - PAD.right, h: H - PAD.top - PAD.bottom };
  const x = (i: number) =>
    PAD.left + (points.length <= 1 ? 0 : (i / (points.length - 1)) * inner.w);
  const y = (v: number) => PAD.top + inner.h - (v / top) * inner.h;
  const line = points
    .map((p, i) => `${i === 0 ? "M" : "L"}${x(i).toFixed(1)},${y(p.value).toFixed(1)}`)
    .join(" ");
  const area = points.length
    ? `${line} L${x(points.length - 1).toFixed(1)},${y(0)} L${x(0).toFixed(1)},${y(0)} Z`
    : "";
  const ticks = [0, top / 2, top];
  const labelAt = [...new Set([0, Math.floor((points.length - 1) / 2), points.length - 1])];
  const active = hover === null ? null : points[hover];

  const move = (event: React.PointerEvent<SVGSVGElement>) => {
    if (points.length === 0) return;
    const box = event.currentTarget.getBoundingClientRect();
    const at = ((event.clientX - box.left) / box.width) * W;
    const index = Math.round(((at - PAD.left) / inner.w) * (points.length - 1));
    setHover(Math.min(points.length - 1, Math.max(0, index)));
  };

  return (
    <figure className="flex min-w-0 flex-col gap-1">
      <figcaption className="text-sm font-medium">{title}</figcaption>
      <div className="relative">
        <svg
          viewBox={`0 0 ${W} ${H}`}
          role="img"
          aria-label={`${title}, ${points.length} days`}
          className="w-full"
          onPointerMove={move}
          onPointerLeave={() => setHover(null)}
        >
          <defs>
            <linearGradient id={gradient} x1="0" x2="0" y1="0" y2="1">
              <stop offset="0%" stopColor="var(--claude)" stopOpacity="0.28" />
              <stop offset="100%" stopColor="var(--claude)" stopOpacity="0.02" />
            </linearGradient>
          </defs>
          {ticks.map((tick) => (
            <g key={tick}>
              <line
                x1={PAD.left}
                x2={W - PAD.right}
                y1={y(tick)}
                y2={y(tick)}
                stroke="var(--border)"
                strokeWidth="1"
              />
              <text
                x={PAD.left - 6}
                y={y(tick) + 3}
                textAnchor="end"
                className="fill-muted-foreground text-[9px]"
              >
                {tick === 0 ? "0" : format(tick)}
              </text>
            </g>
          ))}
          {area ? <path d={area} fill={`url(#${gradient})`} /> : null}
          {line ? <path d={line} fill="none" stroke="var(--claude)" strokeWidth="1.5" /> : null}
          {labelAt.map((index, n) =>
            points[index] ? (
              <text
                key={points[index].day}
                x={x(index)}
                y={H - 6}
                textAnchor={
                  n === 0 ? "start" : n === labelAt.length - 1 && n > 0 ? "end" : "middle"
                }
                className="fill-muted-foreground text-[9px]"
              >
                {dayLabel(points[index].day)}
              </text>
            ) : null,
          )}
          {active && hover !== null ? (
            <g>
              <line
                x1={x(hover)}
                x2={x(hover)}
                y1={PAD.top}
                y2={y(0)}
                stroke="var(--muted-foreground)"
                strokeDasharray="3 3"
              />
              <circle cx={x(hover)} cy={y(active.value)} r="3" fill="var(--claude)" />
            </g>
          ) : null}
        </svg>
        {active ? (
          <div
            role="status"
            className="pointer-events-none absolute top-1 right-2 rounded-md border border-border bg-popover px-2 py-1 text-xs shadow-md"
          >
            <span className="text-muted-foreground">{dayLabel(active.day)}</span>{" "}
            <span className="font-medium">{format(active.value)}</span>
          </div>
        ) : null}
      </div>
      <table className="sr-only">
        <caption>{title}</caption>
        <tbody>
          {points.map((p) => (
            <tr key={p.day}>
              <th scope="row">{p.day}</th>
              <td>{format(p.value)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </figure>
  );
}
