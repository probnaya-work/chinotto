/**
 * DEV-ONLY scaffolding. Not shipped behavior.
 *
 * The Record's real data comes from SQLite through Tauri. When the frontend runs in a plain
 * browser (`npm run dev`) there is no Tauri bridge, so this stands in — purely so the
 * surfaces can be checked against the design without packaging the app each time.
 *
 * The content here is deliberately NOT the design's mockup text. Mockup content is chosen
 * to flatter a layout; this is chosen to stress it: one-word fragments next to a 600-word
 * paragraph, a bare URL, a quotation, a day with 40 fragments in it, years with almost
 * nothing, text with no spaces, and CJK. If a surface survives this it will survive a real
 * record.
 */

import type { Fragment, HeldFragment } from "./recordApi";

export function isTauriShell(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

let seq = 0;
function frag(body: string, capturedAt: Date, extra: Partial<Fragment> = {}): Fragment {
  seq += 1;
  return {
    id: `dev-${seq}`,
    body,
    capturedAt: capturedAt.toISOString(),
    captureMethod: "typed",
    captureOrigin: "desktop",
    correctedAt: null,
    correctionCount: 0,
    legacyEditCount: 0,
    ...extra,
  };
}

const LONG = `Notes on the onboarding call. They don't hate the guide, they hate being made to read it before they're allowed to type. Which means the fix isn't better copy — it's removing the gate entirely and letting the first thing that happens be their own sentence appearing on screen. I keep coming back to the idea that the tutorial is an apology for an interface that doesn't explain itself, and that every minute spent writing one is a minute not spent removing the thing that needed explaining. Worth testing: ship a build with no guide at all, instrument nothing except whether a second fragment gets captured within the first session, and see whether the number moves at all. My guess is it goes up, which would be embarrassing and useful in roughly equal measure.`;

const NO_SPACES =
  "https://example.com/a/very/long/path/that/never/breaks?utm_source=newsletter&utm_campaign=q4&ref=abcdefghijklmnop";

function hoursAgo(h: number): Date {
  return new Date(Date.now() - h * 3_600_000);
}
function daysAgo(d: number, hour = 11): Date {
  const x = new Date();
  x.setDate(x.getDate() - d);
  x.setHours(hour, 17, 0, 0);
  return x;
}

/**
 * DEV-ONLY: `?record=empty` gives a first-open record, `?record=sparse` gives about ten
 * fragments. Both are states the product has to be good at and neither is reachable from a
 * seeded dataset, so they get a switch rather than a separate build.
 */
function devScale(): "empty" | "sparse" | "full" {
  if (typeof location === "undefined") return "full";
  const v = new URLSearchParams(location.search).get("record");
  return v === "empty" ? "empty" : v === "sparse" ? "sparse" : "full";
}

export function devFragments(): Fragment[] {
  const scale = devScale();
  if (scale === "empty") return [];
  if (scale === "sparse") {
    return [
      frag("dinner friday — ask Lena if 8 works, otherwise sat", hoursAgo(1.2)),
      frag("ok", hoursAgo(2.6)),
      frag("the neighbour is playing the same four bars again", hoursAgo(5.1)),
      frag("pharmacy before 6", daysAgo(1, 17)),
      frag("pasta water too salty again. less.", daysAgo(1, 19)),
      frag("moved the desk to the window. worse light, better mood", daysAgo(4)),
      frag("tomatoes, bread, the cheap olive oil", daysAgo(9)),
    ].sort((a, b) => (a.capturedAt < b.capturedAt ? 1 : -1));
  }

  const out: Fragment[] = [];

  // --- the last few hours: D0. Mixed weights on purpose.
  out.push(frag("tired", hoursAgo(0.2)));
  out.push(
    frag("theatlantic.com/ideas/archive/2026/09/second-brain-apps/", hoursAgo(0.6), {
      captureMethod: "url",
    }),
  );
  out.push(frag(LONG, hoursAgo(1.4)));
  out.push(frag("dinner friday — ask Lena if 8 works, otherwise sat", hoursAgo(2.1)));
  out.push(frag("“the map is not the territory” is doing a lot of work in that essay", hoursAgo(3.2)));
  out.push(frag("ok", hoursAgo(4.5)));
  out.push(frag(NO_SPACES, hoursAgo(5.1), { captureMethod: "url" }));
  out.push(frag("読み返すと、やっぱり同じことを考えている", hoursAgo(5.6)));

  // --- earlier today: D1, and enough of it to exercise the overflow count.
  for (let i = 0; i < 26; i++) {
    const h = 7 + i * 0.4;
    out.push(
      frag(
        i % 5 === 0
          ? "no"
          : i % 3 === 0
            ? "the “digital garden” thing again. still don't buy it but the tending metaphor is fine"
            : `a thought that happened earlier today, number ${i + 1}, long enough to need wrapping on a narrow window but not long enough to clamp`,
        hoursAgo(h),
      ),
    );
  }

  // --- yesterday: D2
  out.push(frag("K said “you always start over” — not wrong", daysAgo(1, 21)));
  out.push(frag("pasta water too salty again. less.", daysAgo(1, 19)));
  out.push(frag("pharmacy before 6", daysAgo(1, 9)));

  // --- the ugly material cases, with ids matching devMaterials().
  out.push({ ...frag("", hoursAgo(0.9), { captureMethod: "shared" }), id: "dev-url-title" });
  out.push({ ...frag("", hoursAgo(1.1), { captureMethod: "url" }), id: "dev-url-pending" });
  out.push({ ...frag("worth another look", hoursAgo(1.6), { captureMethod: "url" }), id: "dev-url-failed" });
  out.push({ ...frag("", hoursAgo(2.4), { captureMethod: "url" }), id: "dev-url-long" });
  out.push({
    ...frag("this, but the second half is the whole product problem", hoursAgo(3.1), {
      captureMethod: "shared",
    }),
    id: "dev-url-quote",
  });
  out.push({
    ...frag(
      "right so the return thing — it should only come back if it can show me why, like the actual words, otherwise it's just the app being clever at me. and the same for the guesses, if it's guessing say so, put it in italics or whatever, don't dress it up as a fact. um. the other thing is the phone, I never want to read on the phone, I want to say something and put it down",
      hoursAgo(4.2),
      { captureMethod: "voice" },
    ),
    id: "dev-voice-long",
  });
  out.push({ ...frag("", hoursAgo(4.8), { captureMethod: "voice" }), id: "dev-voice-failed" });
  out.push({ ...frag("ref 4471", hoursAgo(5.3), { captureMethod: "voice" }), id: "dev-voice-corrected" });
  out.push({ ...frag("something said", daysAgo(1, 15), { captureMethod: "voice" }), id: "dev-voice-gone" });
  out.push({ ...frag("", daysAgo(1, 16), { captureMethod: "voice" }), id: "dev-voice-pending" });

  // --- material with no shared vocabulary but obvious shared subject, so the zero-results
  // state (3g) has something real to be close to.
  out.push(frag("boiler guy — Tues between 12 and 3", daysAgo(3, 14)));
  out.push(frag("radiator in the small room is cold at the bottom again. bleed it or call someone", daysAgo(240)));
  out.push(frag("gas safety cert expires march. landlord's problem, my cold.", daysAgo(660)));

  // --- older, this year: D3 (the floor — one line, ellipsis)
  out.push(frag("reread the 2023 note about premature judgment — apparently I've had this thought at least twice", daysAgo(4)));
  out.push(frag("tomatoes, bread, the cheap olive oil, batteries AA", daysAgo(9)));
  out.push(frag("the sea was the colour of a bruise. not a nice one", daysAgo(26)));
  out.push(frag("a legacy fragment with no recorded capture method", daysAgo(40), {
    captureMethod: "imported",
    captureOrigin: "legacy",
  }));
  for (let i = 0; i < 14; i++) out.push(frag(`older material ${i}`, daysAgo(50 + i * 6)));

  // --- prior years, so standing in the past lands in real context rather than a gap.
  // Counts follow devMonthDensity so the year bars and the months agree.
  // The seeded Line, with stable ids so devLines() can chain them.
  out.push({ ...frag("idea: a notebook that doesn't ask what the note is", new Date(2021, 2, 29, 10, 0)), id: "line-2021" });
  out.push({ ...frag("putting something in a folder means deciding what it is before I'm done thinking it", new Date(2023, 2, 14, 23, 41)), id: "line-2023" });
  out.push({ ...frag("same problem with tags. a tag is a folder that's embarrassed about it", new Date(2024, 10, 2, 8, 19)), id: "line-2024" });
  out.push({
    ...frag(
      "Maybe the reason I keep abandoning tools is that they ask me to decide what a thing is before I've finished having it. Filing is a kind of premature judgment.",
      hoursAgo(3.9),
    ),
    id: "line-today",
  });

  const HISTORIC: Record<string, string[]> = {
    "2021-2": [
      "idea: a notebook that doesn't ask what the note is",
      "moved the desk to the window. worse light, better mood",
      "lockdown pasta count: 41",
      "no",
      "…and I think what I actually miss isn't people, it's being interrupted by them",
      "first coffee outside since october. the cup was too hot and I didn't care",
      "vaccine slot 14 apr 11:20, bring the letter",
    ],
    "2021-1": ["the neighbour plays the same four bars every evening at six. I've started to wait for it", "ordered the wrong size again"],
    "2021-3": ["everything smells like paint", "the long way round is the only way I like now"],
    "2020-10": ["everyone is baking. I am not baking.", "the record begins around here"],
    "2023-2": [
      "putting something in a folder means deciding what it is before I'm done thinking it",
      "D's wedding. cried at the wrong part",
      "why does every app want me to name the thing first",
    ],
    "2024-10": ["same problem with tags. a tag is a folder that's embarrassed about it", "new folder structure for the project, again"],
    "2025-4": ["started the long thing about attention again", "ferry back is 16:40", "M's theory: everyone has one recipe"],
  };
  for (const [key, bodies] of Object.entries(HISTORIC)) {
    const [y, m] = key.split("-").map(Number);
    bodies.forEach((body, i) => {
      const d = new Date(y, m, 3 + i * 3, 10 + (i % 7), 12, 0, 0);
      out.push(frag(body, d));
    });
  }

  return out.sort((a, b) => (a.capturedAt < b.capturedAt ? 1 : -1));
}

export function devHeld(): HeldFragment[] {
  if (devScale() !== "full") return [];
  return [{ fragment: frag("boiler guy — Tues between 12 and 3", daysAgo(3)), heldAt: daysAgo(3).toISOString() }];
}

/** Uneven on purpose: a busy year, a sparse year, and one that is almost empty. */
export function devMonthDensity(year: number): number[] {
  const shapes: Record<number, number[]> = {
    2025: [5, 8, 3, 12, 16, 7, 4, 15, 13, 6, 5, 9],
    2024: [10, 3, 0, 8, 9, 14, 5, 0, 7, 11, 16, 8],
    2023: [0, 4, 16, 0, 0, 5, 0, 6, 0, 0, 4, 0],
    2022: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    2021: [0, 0, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0],
  };
  return shapes[year] ?? new Array(12).fill(0);
}

export function devRecordSpan(): [string, string] | null {
  const scale = devScale();
  if (scale === "empty") return null;
  if (scale === "sparse") {
    return [new Date(Date.now() - 20 * 86_400_000).toISOString(), new Date().toISOString()];
  }
  return [new Date("2021-03-29T10:00:00Z").toISOString(), new Date().toISOString()];
}

/**
 * A line that already spans years, so Fragment focus can be checked against 3c without
 * having to build one by hand every reload. Ids match the HISTORIC seeds above.
 */
export function devLines(): Record<string, string[]> {
  const chain = ["line-2021", "line-2023", "line-2024", "line-today"];
  const out: Record<string, string[]> = {};
  for (const id of chain) out[id] = chain;
  return out;
}

/**
 * A Return for the dev browser, shaped exactly like the real one so the surface is
 * exercised rather than approximated. The real selection logic lives in Rust.
 */
export function devReturn(fragments: Fragment[]): import("./recordApi").ReturnValue | null {
  // Silence is valid, and a small record has nothing to return.
  if (devScale() !== "full") return null;
  const old = fragments.find((f) => f.id === "line-2023");
  const because = fragments.find((f) => f.id === "line-today");
  if (!old || !because) return null;
  return {
    id: 1,
    fragment: old,
    reason: "repeated_language",
    because,
    evidence: [
      { kind: "shared_phrase", detail: "deciding what it is before", occurredAt: null, relatedId: null },
      {
        kind: "shared_phrase",
        detail: "decide what a thing is before",
        occurredAt: because.capturedAt,
        relatedId: because.id,
      },
    ],
  };
}

/**
 * DEV-ONLY stand-in for retrieval by meaning. Real guesses come from a local MiniLM model
 * through Tauri; this is a crude word-overlap score, present only so the 3g surface can be
 * exercised in a browser. It is deliberately bad, so it is never mistaken for the real thing.
 */
export function devGuesses(
  fragments: Fragment[],
  query: string,
  exclude: string[],
  limit: number,
): { fragment: Fragment; score: number }[] {
  const terms = query.toLowerCase().split(/\s+/).filter((t) => t.length > 3);
  if (terms.length === 0) return [];
  const RELATED: Record<string, string[]> = {
    heating: ["boiler", "radiator", "cold", "gas"],
    engineer: ["guy", "plumber", "boiler"],
    folder: ["tags", "filing", "organise"],
  };
  const expanded = new Set(terms.flatMap((t) => [t, ...(RELATED[t] ?? [])]));
  return fragments
    .filter((f) => !exclude.includes(f.id))
    .map((f) => {
      const body = f.body.toLowerCase();
      let score = 0;
      for (const t of expanded) if (body.includes(t)) score += 0.3;
      return { fragment: f, score: Math.min(score, 0.95) };
    })
    .filter((g) => g.score >= 0.28)
    .sort((a, b) => b.score - a.score)
    .slice(0, limit);
}

/**
 * DEV-ONLY material, chosen to be ugly rather than tidy: a URL with no title yet, a fetch
 * that failed, a very long unbreakable URL, a shared quotation, a failed transcription, a
 * corrected transcript, a recording whose audio has gone, and a source met more than once.
 */
export function devMaterials(
  ids: string[],
): [import("./recordApi").Encounter[], import("./recordApi").VoiceCapture[]] {
  const enc = (
    fragmentId: string,
    urlRaw: string,
    extra: Partial<import("./recordApi").Encounter> = {},
  ): import("./recordApi").Encounter => ({
    id: Math.abs(hash(fragmentId)),
    fragmentId,
    urlRaw,
    urlKey: urlRaw.replace(/^https?:\/\/(www\.)?/, "").split(/[?#]/)[0],
    sourceApp: null,
    sharedAt: new Date().toISOString(),
    selectedText: null,
    urlCanonical: null,
    domain: urlRaw.replace(/^https?:\/\/(www\.)?/, "").split("/")[0] || null,
    title: null,
    siteName: null,
    enrichmentState: "pending",
    fetchedAt: null,
    failure: null,
    timesMet: 1,
    ...extra,
  });

  const encounters = [
    enc("dev-url-title", "https://theatlantic.com/ideas/archive/2026/09/second-brain-apps/", {
      title: "Why Everyone Suddenly Wants a Second Brain",
      enrichmentState: "ok",
      sourceApp: "com.apple.Safari",
      timesMet: 3,
    }),
    enc("dev-url-pending", "https://theatlantic.com/ideas/archive/2026/09/late-metadata/"),
    enc("dev-url-failed", "https://example.invalid/gone", {
      enrichmentState: "failed",
      failure: "dns failure",
    }),
    enc(
      "dev-url-long",
      "https://example.com/a/very/long/path/that/never/breaks?utm_source=newsletter&utm_campaign=q4&ref=abcdefghijklmnopqrstuvwxyz0123456789",
    ),
    enc("dev-url-quote", "https://maggieappleton.com/garden", {
      title: "A Brief History & Ethos of the Digital Garden",
      enrichmentState: "ok",
      selectedText:
        "the tools promise to remember for you, and then quietly demand that you remember how to use them",
    }),
  ];

  const voice = (
    fragmentId: string,
    durationMs: number,
    extra: Partial<import("./recordApi").VoiceCapture> = {},
  ): import("./recordApi").VoiceCapture => ({
    fragmentId,
    audioPath: `/audio/${fragmentId}.wav`,
    durationMs,
    recordedAt: new Date().toISOString(),
    audioMissing: false,
    machineTranscript: null,
    transcriptState: "ok",
    transcribedAt: new Date().toISOString(),
    failure: null,
    transcriptCorrected: false,
    ...extra,
  });

  const voices = [
    voice("dev-voice-long", 112_000, { machineTranscript: "…" }),
    voice("dev-voice-failed", 6_000, {
      transcriptState: "failed",
      failure: "no speech recognised",
      machineTranscript: null,
    }),
    voice("dev-voice-corrected", 11_000, {
      machineTranscript: "ref four four seven one",
      transcriptCorrected: true,
    }),
    voice("dev-voice-gone", 42_000, { machineTranscript: "something said", audioMissing: true }),
    voice("dev-voice-pending", 3_000, { transcriptState: "pending", machineTranscript: null }),
  ];

  const want = new Set(ids);
  return [
    encounters.filter((e) => want.has(e.fragmentId)),
    voices.filter((v) => want.has(v.fragmentId)),
  ];
}

function hash(s: string): number {
  let h = 0;
  for (let i = 0; i < s.length; i++) h = (h * 31 + s.charCodeAt(i)) | 0;
  return h;
}
