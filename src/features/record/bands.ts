/**
 * The record, grouped by distance.
 *
 * This is `bandsFor()` from the working prototype, transcribed.
 *
 * The important property — and the one the previous implementation did not have — is that
 * this walks the time-sorted list once and opens a new band whenever the level changes. A
 * tier is therefore not a slot: the same tier can appear several times down the column with
 * different labels, and reading order is always strictly chronological. Rendering one fixed
 * block per tier reorders material whenever a tier boundary falls in the middle of a run.
 *
 * Nothing here caps anything. The record shows everything it holds; distance does the
 * compressing, and the surface windows the rendering (see docs/handoff-diff.md
 * §1.1). A cap would be a second, competing idea about what recedes.
 */

import type { Fragment } from "../../lib/recordApi";
import {
  YEARS_LEVEL,
  levelForAnchor,
  levelForRecency,
  referenceFor,
  tierForLevel,
  type Tier,
} from "../../design/tiers";
import { dayLabel, firstLineOf, isSameDay, monthLabel } from "./format";

export interface Anchor {
  year: number;
  /** 0-based, as `Date#getMonth`. */
  month: number;
}

/** A run of fragments at one tier, with the labels covering the days or months it spans. */
export interface MaterialBand {
  kind: "material";
  key: string;
  tier: Tier;
  /** Every distinct label in the band, in the order they appeared. */
  labels: string[];
  items: Fragment[];
}

export interface YearSummary {
  year: number;
  /** Twelve counts, january first. */
  months: number[];
  count: number;
  /** Up to five first lines from across the year. */
  firstLines: string[];
}

/** Everything past six months, folded. One row per year. */
export interface YearsBand {
  kind: "years";
  key: string;
  years: YearSummary[];
}

export type Band = MaterialBand | YearsBand;

export interface BandsInput {
  fragments: Fragment[];
  now: Date;
  /** Standing in a month re-measures distance from there rather than from today. */
  anchor?: Anchor | null;
  /** Find's query, already lowercased. Filters before banding, so bands reflect the result. */
  query?: string | null;
  /**
   * Ids to leave out. Held material is lifted out of the record and shown under the edge,
   * so it must not also appear in its band — but only when you are at the edge: standing in
   * a month or filtering shows the record as it actually is.
   */
  exclude?: ReadonlySet<string> | null;
}

/** Everything a query can match: the words, and anything attached material contributes. */
export function haystack(f: Fragment, extra?: string | null): string {
  return [f.body, extra].filter(Boolean).join(" ").toLowerCase();
}

/**
 * How many rows the band builder will emit for a set of fragments — used by the surface to
 * decide how much to keep mounted without having to build the bands twice.
 */
export function bandRowCount(bands: Band[]): number {
  return bands.reduce(
    (n, b) => n + (b.kind === "years" ? b.years.length : b.items.length),
    0,
  );
}

/**
 * The label a band carries, decided by the level and by the date of the fragment that
 * opened it.
 *
 * D0 never has one — it is now, and saying so would be noise. Standing in a month labels
 * every band with its month and year, because there the year is the thing that locates it.
 */
function labelFor(f: Fragment, level: number, now: Date, anchor: Anchor | null): string | null {
  const at = new Date(f.capturedAt);
  if (anchor) return level === 0 ? null : monthLabel(at, now, true);
  if (level === 0) return null;
  /*
    The size comes from the record's edge; the words come from the clock.

    These must not be the same number. A band can be near *for this record* and still be
    three weeks old, and a column that answers "yesterday" because the tier says D1 would be
    stating something false about when you wrote it. On a record with material in it today
    the two agree exactly, and this is the prototype's rule unchanged.
  */
  const said = levelForRecency(at.getTime(), now.getTime());
  if (said <= 1) return isSameDay(at, now) ? "earlier today" : "yesterday";
  if (said === 2) return dayLabel(at, now);
  return monthLabel(at, now);
}

export function bandsFor({
  fragments,
  now,
  anchor = null,
  query = null,
  exclude = null,
}: BandsInput): Band[] {
  const nowMs = now.getTime();

  let items = exclude ? fragments.filter((f) => !exclude.has(f.id)) : fragments.slice();
  if (query) {
    const q = query.toLowerCase();
    items = items.filter((f) => haystack(f).includes(q));
  }
  // Newest first, always. Everything below depends on this ordering.
  items = items.sort(
    (a, b) => new Date(b.capturedAt).getTime() - new Date(a.capturedAt).getTime(),
  );

  // The edge of the record, which is what distance is measured from. `items` is newest
  // first, so it is the first one — after the filters, so standing in a filtered view
  // measures from what is actually on screen.
  const reference = referenceFor(
    items.length ? new Date(items[0].capturedAt).getTime() : null,
    nowMs,
  );

  const bands: Band[] = [];
  let current: Band | null = null;

  for (const f of items) {
    const at = new Date(f.capturedAt);
    const atMs = at.getTime();
    const level = anchor ? levelForAnchor(atMs, anchor) : levelForRecency(atMs, reference);

    if (level >= YEARS_LEVEL) {
      const year = at.getFullYear();
      if (!current || current.kind !== "years") {
        current = { kind: "years", key: `Y${atMs}`, years: [] };
        bands.push(current);
      }
      let summary = current.years.find((y) => y.year === year);
      if (!summary) {
        summary = { year, months: new Array(12).fill(0), count: 0, firstLines: [] };
        current.years.push(summary);
      }
      summary.months[at.getMonth()] += 1;
      summary.count += 1;
      if (summary.firstLines.length < 5) summary.firstLines.push(firstLineOf(f.body));
      continue;
    }

    const tier = tierForLevel(level);
    // Unreachable: level is 0..5 and 5 was handled above. Skipping rather than throwing
    // keeps a clock or timezone surprise from blanking the whole record.
    if (!tier) continue;

    const label = labelFor(f, level, now, anchor);
    if (!current || current.kind !== "material" || current.tier !== tier) {
      current = { kind: "material", key: `B${level}-${atMs}`, tier, labels: [], items: [] };
      bands.push(current);
    }
    if (label && !current.labels.includes(label)) current.labels.push(label);
    current.items.push(f);
  }

  return bands;
}

/**
 * What a band's label reads as.
 *
 * A band can span several days or months; it names the first three and elides the rest
 * rather than growing. Standing in a month, D0 is the month you are in plus how much is
 * there — the one place a band states a count.
 */
export function bandLabelText(band: MaterialBand, now: Date, anchor: Anchor | null): string {
  if (anchor && band.tier === "d0") {
    const first = band.items[0];
    if (!first) return "";
    return `${monthLabel(new Date(first.capturedAt), now, true)} · ${band.items.length}`;
  }
  const shown = band.labels.slice(0, 3).join(" · ");
  return band.labels.length > 3 ? `${shown} · …` : shown;
}

/** Where a band's label takes you: the month the band opened in. */
export function bandAnchor(band: MaterialBand): Anchor | null {
  const first = band.items[0];
  if (!first) return null;
  const d = new Date(first.capturedAt);
  return { year: d.getFullYear(), month: d.getMonth() };
}

/**
 * A month tick's height in a year row.
 *
 * Absolute, not scaled against the year's own busiest month: the rows are meant to be
 * comparable to each other, so a quiet year has to look quiet next to a loud one. An empty
 * month is drawn at the minimum rather than omitted, so the shape of a year keeps its gaps.
 */
export const YEAR_BAR_MIN = 3;
export const YEAR_BAR_GROWTH = 1.4;
export const YEAR_BAR_CAP = 13;

export function yearBarHeight(count: number): number {
  if (count <= 0) return YEAR_BAR_MIN;
  return YEAR_BAR_MIN + Math.min(YEAR_BAR_CAP, count * YEAR_BAR_GROWTH);
}
