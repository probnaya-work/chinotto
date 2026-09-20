import { describe, expect, it } from "vitest";
import {
  D0_WINDOW_HOURS,
  META,
  TIERS,
  TIER_ORDER,
  YEARS_LEVEL,
  levelForAnchor,
  levelForRecency,
  metaStyle,
  tierForLevel,
  tierStyle,
} from "./tiers";

const ms = (iso: string) => new Date(iso).getTime();

describe("temporal tiers", () => {
  const now = ms("2026-09-19T17:10:00");

  it("puts the last eight hours in D0", () => {
    expect(levelForRecency(ms("2026-09-19T17:02:00"), now)).toBe(0);
    expect(levelForRecency(ms("2026-09-19T09:31:00"), now)).toBe(0);
    // Exactly eight hours ago has already left: the window is half-open.
    expect(levelForRecency(ms("2026-09-19T09:10:00"), now)).toBe(1);
  });

  it("reaches across midnight rather than resetting at it", () => {
    // The whole point of an hours window: something left at 23:00 is still present at 01:00.
    const pastMidnight = ms("2026-09-20T01:00:00");
    expect(levelForRecency(ms("2026-09-19T23:00:00"), pastMidnight)).toBe(0);
  });

  it("drops the rest of today, and yesterday, to D1", () => {
    expect(levelForRecency(ms("2026-09-19T08:40:00"), now)).toBe(1);
    expect(levelForRecency(ms("2026-09-18T21:05:00"), now)).toBe(1);
    expect(levelForRecency(ms("2026-09-18T00:01:00"), now)).toBe(1);
    // The start of yesterday is the edge; a minute earlier is the week.
    expect(levelForRecency(ms("2026-09-17T23:59:00"), now)).toBe(2);
  });

  it("gives the week D2, two months D3 and six months D4", () => {
    expect(levelForRecency(ms("2026-09-16T21:11:00"), now)).toBe(2);
    expect(levelForRecency(ms("2026-09-12T11:03:00"), now)).toBe(3);
    expect(levelForRecency(ms("2026-08-14T19:30:00"), now)).toBe(3);
    expect(levelForRecency(ms("2026-06-21T22:15:00"), now)).toBe(4);
    expect(levelForRecency(ms("2026-04-01T12:00:00"), now)).toBe(4);
  });

  it("folds into years at six months, not at the turn of the year", () => {
    // January of this year is past 180 days, so it folds even though the year has not.
    expect(levelForRecency(ms("2026-01-09T08:30:00"), now)).toBe(YEARS_LEVEL);
    expect(levelForRecency(ms("2023-03-14T23:41:00"), now)).toBe(YEARS_LEVEL);
  });

  it("measures from where you stand, in both directions", () => {
    const anchor = { year: 2024, month: 2 }; // march 2024
    expect(levelForAnchor(ms("2024-03-09T21:30:00"), anchor)).toBe(0);
    expect(levelForAnchor(ms("2024-02-09T21:30:00"), anchor)).toBe(1);
    // Symmetric: a month after reads exactly as a month before.
    expect(levelForAnchor(ms("2024-04-09T21:30:00"), anchor)).toBe(1);
    expect(levelForAnchor(ms("2024-06-14T18:00:00"), anchor)).toBe(2);
    expect(levelForAnchor(ms("2024-11-02T08:19:00"), anchor)).toBe(3);
    expect(levelForAnchor(ms("2023-03-14T23:41:00"), anchor)).toBe(4);
    expect(levelForAnchor(ms("2021-03-29T23:20:00"), anchor)).toBe(YEARS_LEVEL);
  });

  it("names five tiers and nothing past them", () => {
    expect(TIER_ORDER).toEqual(["d0", "d1", "d2", "d3", "d4"]);
    expect(tierForLevel(4)).toBe("d4");
    expect(tierForLevel(YEARS_LEVEL)).toBeNull();
  });
});

describe("the ladder", () => {
  it("narrows monotonically with distance and never widens", () => {
    const widths = TIER_ORDER.map((t) => TIERS[t].wdth);
    expect(widths).toEqual([100, 92, 84, 80, 76]);
    for (let i = 1; i < widths.length; i++) expect(widths[i]).toBeLessThan(widths[i - 1]);
  });

  it("compresses lines with distance: paragraph, 3, 2, 1, 1", () => {
    expect(TIER_ORDER.map((t) => TIERS[t].clamp)).toEqual(["none", 3, 2, 1, 1]);
  });

  it("never renders material below the floor", () => {
    // "nothing below 12px": past D4 a year folds into one row instead of shrinking further.
    const sizes = TIER_ORDER.map((t) => TIERS[t].size);
    expect(sizes[sizes.length - 1]).toBe("var(--size-d4)");
    expect(tierForLevel(5)).toBeNull();
  });

  it("gives only D0 the time gutter, and only D0 no label", () => {
    expect(TIERS.d0.indent).toBe("0");
    expect(TIERS.d0.labelSize).toBeNull();
    for (const t of TIER_ORDER.slice(1)) {
      expect(TIERS[t].indent).toBe("var(--indent)");
      expect(TIERS[t].labelSize).not.toBeNull();
    }
  });

  it("holds the floor tier to one ellipsised line", () => {
    const s = tierStyle("d4");
    expect(s.whiteSpace).toBe("nowrap");
    expect(s.textOverflow).toBe("ellipsis");
  });

  it("clamps the middle tiers without turning them into one line", () => {
    const s = tierStyle("d2");
    expect(s.WebkitLineClamp).toBe(2);
    expect(s.whiteSpace).toBeUndefined();
  });

  it("lets D0 run, and never lets a long token widen the column", () => {
    const s = tierStyle("d0");
    expect(s.WebkitLineClamp).toBeUndefined();
    for (const t of TIER_ORDER) expect(tierStyle(t).overflowWrap).toBe("anywhere");
  });

  it("keeps meta at one width regardless of tier", () => {
    expect(META.wdth).toBe(90);
    expect(metaStyle().fontVariationSettings).toBe("'wdth' 90");
    expect(metaStyle("var(--size-meta-sm)").fontVariationSettings).toBe("'wdth' 90");
  });

  it("states the D0 window once, as a named constant", () => {
    expect(D0_WINDOW_HOURS).toBe(8);
  });
});
