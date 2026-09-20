import { describe, expect, it } from "vitest";
import type { Fragment } from "../../lib/recordApi";
import { assembleTraces, tracesFor, tracesHeading } from "./traces";

const NOW = new Date("2026-09-19T17:10:00").getTime();

let seq = 0;
function frag(body: string, hoursAgo: number, id?: string): Fragment {
  seq += 1;
  return {
    id: id ?? `f${seq}`,
    body,
    capturedAt: new Date(NOW - hoursAgo * 3_600_000).toISOString(),
    captureMethod: "typed",
    captureOrigin: "desktop",
    correctedAt: null,
    correctionCount: 0,
    legacyEditCount: 0,
  };
}

describe("traces", () => {
  it("names a shared run as same words, not a guess", () => {
    const line = [frag("filing is a kind of premature judgment", 1, "focus")];
    const other = frag("still doing premature judgment on every note", 40 * 24, "old");
    const traces = tracesFor(line, [...line, other], new Set());
    expect(traces).toHaveLength(1);
    expect(traces[0].kind).toBe("seen");
    expect(traces[0].phrase).toBe("premature judgment");
    expect(traces[0].why).toBeNull();
  });

  it("offers a guess only when there are shared words and no phrase", () => {
    const line = [frag("tuesday the boiler and the radiator need a heating engineer", 1, "focus")];
    const other = frag("call the engineer, the radiator heating is out, boiler too", 20 * 24, "old");
    const traces = tracesFor(line, [...line, other], new Set());
    expect(traces[0].kind).toBe("guess");
    expect(traces[0].phrase).toBeNull();
    expect(traces[0].why).toMatch(/shared words, no shared phrase/);
  });

  it("does not claim a trace for two fragments that merely share English", () => {
    const line = [frag("pharmacy before six", 1, "focus")];
    const other = frag("tomatoes bread olive oil", 400 * 24, "old");
    expect(tracesFor(line, [...line, other], new Set())).toEqual([]);
  });

  it("omits fragments already on the line, and anything already rejected", () => {
    const a = frag("premature judgment about folders", 1, "a");
    const b = frag("premature judgment again today", 2, "b");
    const c = frag("premature judgment from march", 400 * 24, "c");
    expect(tracesFor([a, b], [a, b, c], new Set(["c"]))).toEqual([]);
  });

  it("states how many were seen and how many were guessed", () => {
    expect(
      tracesHeading([
        {
          fragment: frag("a", 1),
          kind: "seen",
          phrase: "a phrase",
          why: null,
        },
        {
          fragment: frag("b", 2),
          kind: "guess",
          phrase: null,
          why: "3 shared words, no shared phrase",
        },
      ]),
    ).toBe("traces · 1 seen, 1 guessed");
  });

  it("assembles a meaning hit only when it can say why without a score", () => {
    const line = [frag("the heating is off again in the small room", 1, "focus")];
    const semantic = frag("cold radiator, heating failed downstairs", 200 * 24, "mean");
    const assembled = assembleTraces(line, line, new Set(), [], [semantic]);
    expect(assembled).toHaveLength(1);
    expect(assembled[0].kind).toBe("guess");
    expect(assembled[0].why).toMatch(/shared word/);
    expect(assembled[0].why).not.toMatch(/0\.\d+/);
  });

  it("drops a meaning hit that shares no words, rather than citing a score", () => {
    const line = [frag("pharmacy before six", 1, "focus")];
    const semantic = frag("αβγ unrelated glyphs", 200 * 24, "mean");
    expect(assembleTraces(line, line, new Set(), [], [semantic])).toEqual([]);
  });
});
