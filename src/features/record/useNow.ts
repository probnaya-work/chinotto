import { useEffect, useState } from "react";

/**
 * A "now" that is stable between ticks.
 *
 * Every surface here groups material by how far away it is, so every surface needs the
 * current time. Taking it as `now = new Date()` in a default parameter looks harmless and
 * is not: it produces a new object on every render, so any effect or memo depending on it
 * re-runs forever. That is exactly how Find ended up in an infinite render loop.
 *
 * It also has to actually advance. The tiers are defined by elapsed time, so material has
 * to be able to age out of D0 while the window is simply sitting open — a minute is fine
 * for that and costs one re-render.
 */
export function useNow(intervalMs = 60_000): Date {
  const [now, setNow] = useState(() => new Date());

  useEffect(() => {
    const id = setInterval(() => setNow(new Date()), intervalMs);
    return () => clearInterval(id);
  }, [intervalMs]);

  return now;
}
