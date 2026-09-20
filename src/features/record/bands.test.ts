import { describe, expect, it } from "vitest";
import type { Fragment } from "../../lib/recordApi";
import { bandAnchor, bandLabelText, bandsFor, yearBarHeight, type MaterialBand } from "./bands";

const NOW = new Date("2026-09-19T17:10:00");

let seq = 0;
function frag(iso: string, body = "a fragment"): Fragment {
  seq += 1;
  return {
    id: `f${seq}`,
    body,
    capturedAt: new Date(iso).toISOString(),
    captureMethod: "typed",
    captureOrigin: "desktop",
    correctedAt: null,
    correctionCount: 0,
    legacyEditCount: 0,
  };
}

const shape = (bands: ReturnType<typeof bandsFor>) =>
  bands.map((b) => (b.kind === "years" ? `years(${b.years.length})` : `${b.tier}(${b.items.length})`));

describe("banding the record", () => {
  it("walks time once rather than filling one slot per tier", () => {
    // The order is the point: a tier that recurs must open a second band where it recurs,
    // not merge back into the first one, or the column stops being chronological.
    const bands = bandsFor({
      fragments: [
        frag("2026-09-19T17:02:00"),
        frag("2026-09-19T08:40:00"),
        frag("2026-09-16T21:11:00"),
        frag("2026-09-12T11:03:00"),
      ],
      now: NOW,
    });
    expect(shape(bands)).toEqual(["d0(1)", "d1(1)", "d2(1)", "d3(1)"]);
  });

  it("keeps newest first even when the input is not sorted", () => {
    const bands = bandsFor({
      fragments: [frag("2026-09-19T09:00:00", "older"), frag("2026-09-19T17:02:00", "newer")],
      now: NOW,
    });
    expect((bands[0] as MaterialBand).items[0].body).toBe("newer");
  });

  it("labels D1 by whether it is still today", () => {
    const bands = bandsFor({
      fragments: [frag("2026-09-19T08:40:00"), frag("2026-09-18T21:05:00")],
      now: NOW,
    });
    expect((bands[0] as MaterialBand).labels).toEqual(["earlier today", "yesterday"]);
  });

  it("never labels D0", () => {
    const bands = bandsFor({ fragments: [frag("2026-09-19T17:02:00")], now: NOW });
    expect((bands[0] as MaterialBand).labels).toEqual([]);
    expect(bandLabelText(bands[0] as MaterialBand, NOW, null)).toBe("");
  });

  it("names at most three of the days a band spans, then elides", () => {
    const band = bandsFor({
      fragments: [
        frag("2026-09-16T10:00:00"),
        frag("2026-09-15T10:00:00"),
        frag("2026-09-14T10:00:00"),
        frag("2026-09-13T10:00:00"),
      ],
      now: NOW,
    })[0] as MaterialBand;
    expect(band.labels).toHaveLength(4);
    expect(bandLabelText(band, NOW, null)).toBe("wed 16 · tue 15 · mon 14 · …");
  });

  it("folds everything past six months into years, oldest tier first", () => {
    const bands = bandsFor({
      fragments: [
        frag("2026-01-09T08:30:00"),
        frag("2025-05-02T07:50:00"),
        frag("2025-11-20T12:00:00"),
      ],
      now: NOW,
    });
    expect(shape(bands)).toEqual(["years(2)"]);
    const years = bands[0].kind === "years" ? bands[0].years : [];
    expect(years.map((y) => y.year)).toEqual([2026, 2025]);
    expect(years[0].count).toBe(1);
    expect(years[0].months[0]).toBe(1); // january
    expect(years[1].count).toBe(2);
  });

  it("keeps at most five first lines per year", () => {
    const bands = bandsFor({
      fragments: Array.from({ length: 9 }, (_, i) =>
        frag(`2021-03-0${i + 1}T10:00:00`, `line ${i}`),
      ),
      now: NOW,
    });
    const years = bands[0].kind === "years" ? bands[0].years : [];
    expect(years[0].count).toBe(9);
    expect(years[0].firstLines).toHaveLength(5);
  });

  it("truncates a first line rather than letting a paragraph into a year row", () => {
    const bands = bandsFor({
      fragments: [frag("2021-03-01T10:00:00", "x".repeat(400))],
      now: NOW,
    });
    const years = bands[0].kind === "years" ? bands[0].years : [];
    expect(years[0].firstLines[0]).toHaveLength(90);
  });

  it("takes only the first line of a multi-paragraph fragment", () => {
    const bands = bandsFor({
      fragments: [frag("2021-03-01T10:00:00", "the first line\n\nand a second paragraph")],
      now: NOW,
    });
    const years = bands[0].kind === "years" ? bands[0].years : [];
    expect(years[0].firstLines[0]).toBe("the first line");
  });

  it("measures from where you stand when standing in a month", () => {
    const anchor = { year: 2024, month: 2 };
    const bands = bandsFor({
      fragments: [
        frag("2024-03-22T12:10:00"),
        frag("2024-03-09T21:30:00"),
        frag("2024-01-05T10:30:00"),
        frag("2021-03-29T23:20:00"),
      ],
      now: NOW,
      anchor,
    });
    expect(shape(bands)).toEqual(["d0(2)", "d2(1)", "years(1)"]);
    // Standing, D0 states the month and how much is in it — the one band that counts.
    expect(bandLabelText(bands[0] as MaterialBand, NOW, anchor)).toBe("mar 2024 · 2");
    // And every other band carries its year, because that is what locates it there.
    expect((bands[1] as MaterialBand).labels).toEqual(["jan 2024"]);
  });

  it("filters before banding, so the bands describe the result and not the record", () => {
    const bands = bandsFor({
      fragments: [
        frag("2026-09-19T17:02:00", "tired"),
        frag("2026-09-19T16:20:00", "notes on the onboarding call"),
        frag("2026-09-16T21:11:00", "the map is not the territory"),
      ],
      now: NOW,
      query: "the",
    });
    expect(shape(bands)).toEqual(["d0(1)", "d2(1)"]);
  });

  it("matches case-insensitively", () => {
    const bands = bandsFor({
      fragments: [frag("2026-09-19T17:02:00", "Notes On The Onboarding Call")],
      now: NOW,
      query: "ONBOARDING",
    });
    expect(shape(bands)).toEqual(["d0(1)"]);
  });

  it("lifts held material out of the record, but only when asked to", () => {
    const held = frag("2026-09-18T14:20:00", "boiler guy");
    const rest = frag("2026-09-18T13:48:00", "pasta water too salty again");
    const atEdge = bandsFor({
      fragments: [held, rest],
      now: NOW,
      exclude: new Set([held.id]),
    });
    expect(shape(atEdge)).toEqual(["d1(1)"]);
    // Standing or filtering shows the record as it actually is.
    const standing = bandsFor({ fragments: [held, rest], now: NOW });
    expect(shape(standing)).toEqual(["d1(2)"]);
  });

  it("returns nothing for an empty record rather than an empty band", () => {
    expect(bandsFor({ fragments: [], now: NOW })).toEqual([]);
  });

  it("survives a burst: forty fragments in one hour stay one band", () => {
    const burst = Array.from({ length: 40 }, (_, i) =>
      frag(`2026-09-19T16:${String(i % 60).padStart(2, "0")}:00`, `burst ${i}`),
    );
    const bands = bandsFor({ fragments: burst, now: NOW });
    expect(shape(bands)).toEqual(["d0(40)"]);
  });

  it("sends a band's label to the month it opened in", () => {
    const band = bandsFor({
      fragments: [frag("2026-08-14T19:30:00"), frag("2026-08-01T10:00:00")],
      now: NOW,
    })[0] as MaterialBand;
    expect(bandAnchor(band)).toEqual({ year: 2026, month: 7 });
  });
});

describe("year bars", () => {
  it("draws an empty month rather than omitting it", () => {
    expect(yearBarHeight(0)).toBe(3);
  });

  it("grows with the count and then stops, so one loud month cannot dwarf a year", () => {
    expect(yearBarHeight(1)).toBeCloseTo(4.4);
    expect(yearBarHeight(5)).toBeCloseTo(10);
    expect(yearBarHeight(10)).toBe(16);
    expect(yearBarHeight(5000)).toBe(16);
  });

  it("is absolute, so two years can be compared", () => {
    // A quiet year has to look quiet next to a loud one; scaling each year against its own
    // peak made every year look equally busy.
    expect(yearBarHeight(2)).toBeLessThan(yearBarHeight(8));
  });
});
