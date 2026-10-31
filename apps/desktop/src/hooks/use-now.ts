import { useEffect, useState } from "react";

/** The time in milliseconds, read again every `every` while `active`. Nothing runs when it is not,
 * so a screen with nothing to count does not wake up each second. */
export function useNow(active: boolean, every = 1000): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!active) return;
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), every);
    return () => clearInterval(timer);
  }, [active, every]);
  return now;
}
