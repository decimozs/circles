import { cn } from "@circles/ui/utils";
import type { CSSProperties } from "react";

/** How many colours there are to pick from. They are `--orb-1` to `--orb-12` in the tokens. */
const PALETTE = 12;
const PATHS = ["orb-a", "orb-b", "orb-c"] as const;

/** A small, fast random number generator that gives the same numbers for the same seed. */
function generator(seed: string) {
  let h = 1779033703 ^ seed.length;
  for (let i = 0; i < seed.length; i++) {
    h = Math.imul(h ^ seed.charCodeAt(i), 3432918353);
    h = (h << 13) | (h >>> 19);
  }
  let a = h >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export type Blob = {
  color: string;
  size: number;
  left: number;
  top: number;
  path: (typeof PATHS)[number];
  seconds: number;
  delay: number;
};

export type Orb = { base: string; blobs: Blob[] };

/**
 * The look of the orb for a seed, such as a Composer's id. The same seed always gives the same
 * orb, and different seeds give different ones: a base of two colours, and three or four soft
 * blobs of other colours that drift over it.
 */
export function orbFor(seed: string): Orb {
  const random = generator(seed);
  const pool = Array.from({ length: PALETTE }, (_, i) => i + 1);
  const pick = () => pool.splice(Math.floor(random() * pool.length), 1)[0] as number;
  const var_ = (n: number) => `var(--orb-${n})`;
  const angle = Math.round(random() * 360);
  const base = `linear-gradient(${angle}deg, ${var_(pick())}, ${var_(pick())})`;
  const count = 3 + Math.floor(random() * 2);
  const blobs: Blob[] = Array.from({ length: count }, (_, i) => ({
    color: var_(pick()),
    size: 55 + Math.round(random() * 35),
    left: Math.round(random() * 70) - 10,
    top: Math.round(random() * 70) - 10,
    path: PATHS[(i + Math.floor(random() * 3)) % 3] as Blob["path"],
    seconds: 8 + Math.round(random() * 8),
    delay: -Math.round(random() * 16),
  }));
  return { base, blobs };
}

/**
 * A round, softly moving gradient that stands for a Composer. It is made from the Composer's id,
 * so it never changes, and no two look alike.
 */
export function ComposerOrb({
  seed,
  className,
  label,
}: {
  seed: string;
  className?: string;
  /** Said by screen readers. Without it the orb is decoration. */
  label?: string;
}) {
  const orb = orbFor(seed);
  return (
    <span
      data-testid="composer-orb"
      {...(label ? { role: "img", "aria-label": label } : { "aria-hidden": true })}
      style={{
        background: orb.base,
        // A little depth at the edge, like a sphere, without any colour of its own.
        boxShadow:
          "inset 0 -2px 3px color-mix(in oklab, black 28%, transparent), inset 0 1px 2px color-mix(in oklab, white 30%, transparent)",
      }}
      className={cn(
        "relative inline-block size-4 shrink-0 overflow-hidden rounded-full",
        className,
      )}
    >
      {orb.blobs.map((blob, i) => {
        const style: CSSProperties = {
          width: `${blob.size}%`,
          height: `${blob.size}%`,
          left: `${blob.left}%`,
          top: `${blob.top}%`,
          background: blob.color,
          animation: `${blob.path} ${blob.seconds}s ease-in-out ${blob.delay}s infinite alternate`,
          willChange: "transform",
        };
        return (
          <span
            // biome-ignore lint/suspicious/noArrayIndexKey: a fixed few blobs
            key={i}
            data-testid="composer-orb-blob"
            className="absolute rounded-full opacity-90 blur-[3px]"
            style={style}
          />
        );
      })}
    </span>
  );
}
