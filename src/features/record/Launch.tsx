/**
 * Launch.
 *
 * A brief identity moment while the local Record loads. Not onboarding: no key press, no
 * explanation, and — the part that is easy to get wrong — **no artificial delay**. The hold
 * is what is left of 2.6 seconds after loading, so a slow record exits immediately and a
 * fast one is the only case where anything waits at all.
 *
 * The animation is the identity rather than decoration. The three moments arrive out of the
 * record — the near one first and fast, the far one last and slow — and the ring closes
 * around them once they are in place. Then the word rises out of its clip. Leaving, the dots
 * fall back the way they came, the ring opens, the word lifts, and the record rises
 * underneath.
 *
 * Before the record resolves the lockup is rendered invisible but already centred, so
 * nothing flashes at the top of the window and nothing moves when it appears.
 */

import { useEffect, useRef, useState } from "react";

export type LaunchPhase = "mark" | "leaving" | "done";

/** What is left of this after loading is the hold. It is a ceiling, never a floor. */
export const LAUNCH_HOLD_MS = 2600;
export const LAUNCH_EXIT_MS = 520;
/** Reduced motion still marks the moment, but as a held lockup rather than a performance. */
export const LAUNCH_HOLD_REDUCED_MS = 500;
export const LAUNCH_EXIT_REDUCED_MS = 200;

const EASE = "cubic-bezier(.16,.84,.3,1)";

/**
 * How long the lockup still has to hold, given how long loading already took.
 *
 * The whole rule in one function, so it can be checked rather than trusted: time already
 * spent loading counts against the hold, and a record that took longer than the window
 * exits at once. It is a ceiling, never a floor — launch can only ever make a *fast* start
 * slower, never a slow one.
 */
export function launchHoldMs(loadedInMs: number, reduced = false): number {
  const wanted = reduced ? LAUNCH_HOLD_REDUCED_MS : LAUNCH_HOLD_MS;
  return Math.max(0, wanted - loadedInMs);
}

export function launchExitMs(reduced = false): number {
  return reduced ? LAUNCH_EXIT_REDUCED_MS : LAUNCH_EXIT_MS;
}

function prefersReducedMotion(): boolean {
  try {
    return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  } catch {
    return false;
  }
}

/**
 * Drives the launch state.
 *
 * `ready` is the record having resolved — not a timer. Everything downstream keys off that,
 * which is what keeps a slow first read from being made slower.
 */
export function useLaunch(ready: boolean, skip = false): LaunchPhase {
  const [phase, setPhase] = useState<LaunchPhase>(skip ? "done" : "mark");
  const startedAt = useRef(Date.now());
  const armed = useRef(false);
  const timers = useRef<ReturnType<typeof setTimeout>[]>([]);

  useEffect(() => {
    if (skip || !ready || armed.current) return;
    armed.current = true;

    const reduced = prefersReducedMotion();
    const hold = launchHoldMs(Date.now() - startedAt.current, reduced);
    const exit = launchExitMs(reduced);

    const a = setTimeout(() => {
      setPhase("leaving");
      const b = setTimeout(() => setPhase("done"), exit);
      timers.current.push(b);
    }, hold);
    timers.current.push(a);
  }, [ready, skip]);

  useEffect(() => () => timers.current.forEach(clearTimeout), []);

  return phase;
}

export function Launch({ phase, visible }: { phase: LaunchPhase; visible: boolean }) {
  if (phase === "done") return null;
  const leaving = phase === "leaving";
  const reduced = prefersReducedMotion();

  const dot = (n: 1 | 2 | 3, delay: number, duration: number) =>
    reduced
      ? {}
      : {
          transformOrigin: "32px 32px",
          animation: leaving
            ? `chinotto-scatter${n} .4s ease-in ${(3 - n) * 0.05}s both`
            : `chinotto-arrive${n} ${duration}s ${EASE} ${delay}s both`,
        };

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 40,
        background: "var(--surface)",
        boxSizing: "border-box",
        overflow: "hidden",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        // While it is leaving it must not swallow a click meant for the record underneath.
        pointerEvents: leaving ? "none" : "auto",
        // Invisible but already centred until the record resolves: nothing flashes, and
        // nothing moves into place when it appears.
        opacity: visible ? 1 : 0,
      }}
    >
      <div style={{ display: "flex", flexDirection: "column", alignItems: "center" }}>
        <svg
          width={132}
          height={132}
          viewBox="0 0 64 64"
          fill="none"
          role="img"
          aria-label="Chinotto"
          style={{ color: "var(--ink)", flex: "none", marginBottom: 32, overflow: "visible" }}
        >
          {/*
            1.6 rather than the size ladder's 2.5. At 132px the ring is 3.3 device pixels
            and the mark is read as a shape rather than an icon, which is the one place the
            prototype draws it lighter. See docs/handoff-diff.md §1.8.
          */}
          <circle
            cx="32"
            cy="32"
            r="28"
            stroke="currentColor"
            strokeWidth="1.6"
            strokeLinecap="round"
            style={
              reduced
                ? { transformOrigin: "32px 32px", transform: "rotate(-90deg)" }
                : {
                    transformOrigin: "32px 32px",
                    transform: "rotate(-90deg)",
                    animation: leaving
                      ? "chinotto-ringopen .44s ease-in both"
                      : `chinotto-ringclose .9s ${EASE} 1.02s both`,
                  }
            }
          />
          <circle cx="32" cy="23" r="8" fill="currentColor" style={dot(1, 0.12, 0.62)} />
          <circle cx="32" cy="38" r="4.5" fill="currentColor" style={dot(2, 0.3, 0.8)} />
          <circle cx="32" cy="47.5" r="2.5" fill="currentColor" style={dot(3, 0.46, 0.98)} />
        </svg>

        {/* The word rises out of the clip rather than fading: it arrives, it does not appear. */}
        <div style={{ overflow: "hidden", paddingBottom: 6 }}>
          <span
            style={{
              display: "block",
              fontSize: "var(--size-wordmark-launch)",
              lineHeight: 1,
              letterSpacing: "var(--track-wordmark-launch)",
              fontVariationSettings: "'wdth' 96",
              fontWeight: 500,
              color: "var(--ink)",
              animation: reduced
                ? "none"
                : leaving
                  ? "chinotto-wordout .38s ease-in both"
                  : `chinotto-riseword .8s ${EASE} 1.5s both`,
            }}
          >
            chinotto
          </span>
        </div>
      </div>
    </div>
  );
}
