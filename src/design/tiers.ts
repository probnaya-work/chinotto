/**
 * Temporal tiers.
 *
 * Distance decides everything about how a fragment reads: its size, its colour, its width
 * axis and how many lines it is allowed. Nothing else does — not kind, not length, not
 * importance. There are five tiers, D0…D4, and past D4 a year folds into one row, which is
 * not a tier.
 *
 * The tiers are FIXED and NAMED. There is no interpolation between them, because the
 * prototype's `STY` table defines five and states no rule for anything in between.
 *
 * Source of truth: `Chinotto Desktop.dc.html`, the `STY` table and `bandsFor()`'s `level()`.
 */

import type { CSSProperties } from "react";

export type Tier = "d0" | "d1" | "d2" | "d3" | "d4";

/** The tiers in reading order, near to far. */
export const TIER_ORDER: readonly Tier[] = ["d0", "d1", "d2", "d3", "d4"] as const;

/** `bandsFor()` speaks in levels; 5 and above is the years band, which is not a tier. */
export const YEARS_LEVEL = 5;

export function tierForLevel(level: number): Tier | null {
  return TIER_ORDER[level] ?? null;
}

export interface TierStyle {
  /** CSS font-size. */
  size: string;
  lineHeight: number;
  /** Archivo's wdth axis. Material narrows as it recedes; meta does not (see META). */
  wdth: number;
  /** Ink token for material at this tier. */
  ink: string;
  /** How far text is allowed to run before it compresses: paragraph → 3 → 2 → 1 → 1. */
  clamp: number | "none";
  /** The gap between rows inside this tier. */
  gap: string;
  /** The space above the band. */
  bandTop: string;
  /** How far the band hangs in from the column's left edge. D0 keeps the time gutter. */
  indent: string;
  /** Size of the band's own label. D0 never has one. */
  labelSize: string | null;
  letterSpacing?: string;
}

export const TIERS: Record<Tier, TierStyle> = {
  // The last eight hours. Full paragraphs, full width, brightest ink, and the only tier
  // with a time gutter, hover verbs and a row wash.
  d0: {
    size: "var(--size-d0)",
    lineHeight: 1.24,
    wdth: 100,
    ink: "var(--ink)",
    clamp: "none",
    gap: "var(--gap-d0)",
    bandTop: "var(--band-top-d0)",
    indent: "0",
    labelSize: null,
    letterSpacing: "var(--track-d0)",
  },
  // Earlier today, or yesterday.
  d1: {
    size: "var(--size-d1)",
    lineHeight: 1.26,
    wdth: 92,
    ink: "var(--ink-near)",
    clamp: 3,
    gap: "var(--gap-d1)",
    bandTop: "var(--band-top-d1)",
    indent: "var(--indent)",
    labelSize: "var(--size-meta-sm)",
  },
  // The last week.
  d2: {
    size: "var(--size-d2)",
    lineHeight: 1.3,
    wdth: 84,
    ink: "var(--ink-far)",
    clamp: 2,
    gap: "var(--gap-d2)",
    bandTop: "var(--band-top-d2)",
    indent: "var(--indent)",
    labelSize: "var(--size-meta-xs)",
  },
  // The last two months.
  d3: {
    size: "var(--size-d3)",
    lineHeight: 1.3,
    wdth: 80,
    ink: "var(--ink-dim)",
    clamp: 1,
    gap: "var(--gap-d3)",
    bandTop: "var(--band-top-d3)",
    indent: "var(--indent)",
    labelSize: "var(--size-tier-label-far)",
  },
  // The last six months. The floor: 12px, one line, ellipsis. Nothing goes below this —
  // past here a year folds into a single row instead.
  d4: {
    size: "var(--size-d4)",
    lineHeight: 1.3,
    wdth: 76,
    ink: "var(--meta)",
    clamp: 1,
    gap: "var(--gap-d4)",
    bandTop: "var(--band-top-d4)",
    indent: "var(--indent)",
    labelSize: "var(--size-tier-label-far)",
  },
};

/**
 * Meta is a register, not a distance: time labels, provenance, affordances and counts sit
 * at wdth 90 at every size and in every tier. Keeping this separate from TIERS is what
 * stops "12px" from being ambiguous between far material and a time label.
 */
export const META = {
  wdth: 90,
  ink: "var(--meta)",
} as const;

/**
 * Named exceptions the prototype draws deliberately, rather than tiers.
 */
export const EXCEPTIONS = {
  /** Held material is WIDER than the tier below it — it is being kept present. */
  held: { size: "var(--size-held)", wdth: 96, ink: "var(--ink-near)", lineHeight: 1.3 },
  /** A moment inside a Line, and Find's meaning-guesses. */
  moment: { size: "var(--size-moment)", wdth: 94, ink: "var(--ink-near)", lineHeight: 1.28 },
  /** A Return's material, which stays large: it is here now, not receding. */
  returned: { size: "var(--size-return)", wdth: 100, ink: "var(--ink)", lineHeight: 1.22 },
  /** A Return's reason, and utility surfaces' secondary measure. */
  evidence: { size: "var(--size-evidence)", wdth: 92, ink: "var(--ink-far)", lineHeight: 1.45 },
} as const;

/**
 * The tier windows, as the prototype's `level()` states them.
 *
 * D0 is the last eight hours, not the calendar day, so something left at 23:00 is still
 * present at 01:00. D1 reaches back to the start of yesterday. Everything past six months
 * folds into years — not at the calendar-year boundary.
 */
export const D0_WINDOW_HOURS = 8;
export const D2_WINDOW_DAYS = 7;
export const D3_WINDOW_DAYS = 60;
export const D4_WINDOW_DAYS = 180;

const HOUR = 3_600_000;
const DAY = 86_400_000;

/**
 * Where the record's distances are measured from.
 *
 * Not the clock — the record's own edge, which is where your writing actually stops.
 *
 * The prototype measures from `now`, and cannot show what that costs: its corpus is built
 * relative to its own `NOW`, so it always has today's material in it. A real record that has
 * been left alone for three weeks recedes *whole* — every tier collapses onto the floor
 * together, and the column becomes one wall of 12px at the moment you come back to it. The
 * ladder is meant to say "this is near and that is far", and a ladder with everything on
 * the bottom rung says nothing.
 *
 * Reading from the edge keeps the ladder's shape: the newest thing you have is as near as
 * it was when you wrote it, and everything behind it recedes away from it. What it costs is
 * that a fragment can grow back toward D0 as older material ages past it — so the words are
 * still taken from the clock (see `labelFor`), and a row's gutter still says the day it was
 * actually written.
 *
 * Never later than now: a fragment dated in the future — a clock moved back, a phone in the
 * wrong timezone — must not drag the whole record forward with it.
 */
export function referenceFor(newestCapturedAt: number | null, now: number): number {
  if (newestCapturedAt === null || Number.isNaN(newestCapturedAt)) return now;
  return Math.min(now, newestCapturedAt);
}

/**
 * Which level a fragment sits at, counted from `reference`. 5 means the years band.
 *
 * This is `level()` from the prototype, transcribed. It is deliberately a free function on
 * timestamps rather than a method on anything, so the band builder and its tests can call
 * it with a fixed clock — and so the reference can be the record's edge rather than now.
 */
export function levelForRecency(capturedAt: number, now: number): number {
  const dt = now - capturedAt;
  if (dt < D0_WINDOW_HOURS * HOUR) return 0;

  const n = new Date(now);
  const todayStart = new Date(n.getFullYear(), n.getMonth(), n.getDate()).getTime();
  if (capturedAt >= todayStart - DAY) return 1;

  if (dt < D2_WINDOW_DAYS * DAY) return 2;
  if (dt < D3_WINDOW_DAYS * DAY) return 3;
  if (dt < D4_WINDOW_DAYS * DAY) return 4;
  return YEARS_LEVEL;
}

/**
 * Which level a fragment sits at while you are standing in a month.
 *
 * Distance is measured from where you stand, not from today, so the month you are in reads
 * as D0 and everything else recedes away from it in both directions.
 */
export function levelForAnchor(capturedAt: number, anchor: { year: number; month: number }): number {
  const d = new Date(capturedAt);
  const months = Math.abs(
    anchor.year * 12 + anchor.month - (d.getFullYear() * 12 + d.getMonth()),
  );
  if (months === 0) return 0;
  if (months === 1) return 1;
  if (months <= 3) return 2;
  if (months <= 8) return 3;
  if (months <= 14) return 4;
  return YEARS_LEVEL;
}

/** Inline style for material at a tier. Keeps wdth and clamp from drifting apart. */
export function tierStyle(tier: Tier): CSSProperties {
  const t = TIERS[tier];
  const base: CSSProperties = {
    fontSize: t.size,
    lineHeight: t.lineHeight,
    color: t.ink,
    fontVariationSettings: `'wdth' ${t.wdth}`,
    letterSpacing: t.letterSpacing,
    // A pasted URL or a long unbroken token must never widen the column and push the
    // surface sideways. Material wraps; the layout does not move.
    overflowWrap: "anywhere",
  };
  if (t.clamp === 1) {
    // The floor: one line, ellipsis, never wrapping.
    return { ...base, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" };
  }
  if (typeof t.clamp === "number") {
    return {
      ...base,
      display: "-webkit-box",
      WebkitLineClamp: t.clamp,
      WebkitBoxOrient: "vertical",
      overflow: "hidden",
    };
  }
  return { ...base, textWrap: "pretty" } as CSSProperties;
}

/** Inline style for meta text at any tier. */
export function metaStyle(size: string = "var(--size-meta)"): CSSProperties {
  return {
    fontSize: size,
    color: META.ink,
    fontVariationSettings: `'wdth' ${META.wdth}`,
  };
}

/** A band's own label: uppercase, tracked, meta, and clickable — it takes you to that month. */
export function bandLabelStyle(tier: Tier): CSSProperties {
  return {
    ...metaStyle(TIERS[tier].labelSize ?? "var(--size-meta-sm)"),
    letterSpacing: "var(--track-tier-label)",
    textTransform: "uppercase",
    cursor: "pointer",
  };
}
