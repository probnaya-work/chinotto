import { describe, expect, it } from "vitest";
import {
  LAUNCH_EXIT_MS,
  LAUNCH_EXIT_REDUCED_MS,
  LAUNCH_HOLD_MS,
  LAUNCH_HOLD_REDUCED_MS,
  launchExitMs,
  launchHoldMs,
} from "./Launch";

describe("the launch hold", () => {
  it("holds the rest of the window when the record was quick", () => {
    expect(launchHoldMs(0)).toBe(LAUNCH_HOLD_MS);
    expect(launchHoldMs(600)).toBe(LAUNCH_HOLD_MS - 600);
  });

  it("never extends a slow record", () => {
    // The rule the design states outright: if loading took longer than the hold, the
    // lockup leaves at once. Launch may only ever make a fast start slower.
    expect(launchHoldMs(LAUNCH_HOLD_MS)).toBe(0);
    expect(launchHoldMs(LAUNCH_HOLD_MS + 1)).toBe(0);
    expect(launchHoldMs(30_000)).toBe(0);
  });

  it("is a ceiling, so the total is never more than the window", () => {
    for (const load of [0, 100, 1_000, 2_599, 2_600, 10_000]) {
      expect(load + launchHoldMs(load)).toBeLessThanOrEqual(
        Math.max(LAUNCH_HOLD_MS, load),
      );
    }
  });

  it("marks the moment more briefly under reduced motion, and still never waits", () => {
    expect(launchHoldMs(0, true)).toBe(LAUNCH_HOLD_REDUCED_MS);
    expect(launchHoldMs(LAUNCH_HOLD_REDUCED_MS + 10, true)).toBe(0);
    expect(launchExitMs(true)).toBe(LAUNCH_EXIT_REDUCED_MS);
    expect(launchExitMs()).toBe(LAUNCH_EXIT_MS);
  });

  it("keeps the reduced-motion path shorter than the full one", () => {
    expect(LAUNCH_HOLD_REDUCED_MS).toBeLessThan(LAUNCH_HOLD_MS);
    expect(LAUNCH_EXIT_REDUCED_MS).toBeLessThan(LAUNCH_EXIT_MS);
  });
});
