/**
 * Traces — what else in the record touches this one.
 *
 * Two kinds, and the difference between them is the whole point:
 *
 *   `same words` — a run of shared language actually appears in both. The product can state
 *                  this, and show the phrase, because it is a fact about the text.
 *   `a guess`    — no shared phrase, but enough shared vocabulary to be worth offering. It
 *                  says it is a guess, it says why in countable terms, and it offers `yes`
 *                  and `not this` so the person decides.
 *
 * A guess's reason is "N shared words, no shared phrase" — never a similarity score.
 * A number nobody can check is not an explanation, and the product's rule is that inferred
 * results must be distinguishable and explainable. Counting words is both.
 *
 * This runs over material already in memory. It never queries, and it is never on the path
 * of anything the person is waiting for.
 */

import type { Fragment } from "../../lib/recordApi";
import { sharedCount, sharedRun, tokenize, type Token } from "./words";

export interface Trace {
  fragment: Fragment;
  kind: "seen" | "guess";
  /** The shared run, for `seen`. Null for a guess — there is no phrase to point at. */
  phrase: string | null;
  /** "4 shared words, no shared phrase". Only for a guess. */
  why: string | null;
}

/** Below this, two fragments simply both happen to be in English. */
const MIN_SHARED_WORDS_FOR_GUESS = 3;

/** Shorter than this and there is nothing to compare. */
const MIN_TOKENS = 3;

/** How many traces a surface will show. Past this it stops being a trace and becomes a list. */
const MAX_TRACES = 6;

export function tracesFor(
  line: Fragment[],
  corpus: Fragment[],
  rejected: ReadonlySet<string>,
): Trace[] {
  const inLine = new Set(line.map((m) => m.id));
  const lineTokens: Token[][] = line.map((m) => tokenize(m.body));
  const out: Trace[] = [];
  // Two fragments with identical words are one trace, not two — the record legitimately
  // holds the same sentence twice, and saying so twice is noise.
  const seenText = new Set<string>();

  for (const g of corpus) {
    if (inLine.has(g.id) || rejected.has(g.id)) continue;
    const key = g.body.toLowerCase();
    if (seenText.has(key)) continue;
    seenText.add(key);

    const tg = tokenize(g.body);
    if (tg.length < MIN_TOKENS) continue;

    let bestRun: { length: number; a: string } | null = null;
    let bestCount = 0;
    for (const lt of lineTokens) {
      const run = sharedRun(tg, lt);
      if (run && (!bestRun || run.length > bestRun.length)) bestRun = run;
      bestCount = Math.max(bestCount, sharedCount(tg, lt));
    }

    if (bestRun) {
      out.push({ fragment: g, kind: "seen", phrase: bestRun.a, why: null });
    } else if (bestCount >= MIN_SHARED_WORDS_FOR_GUESS) {
      out.push({
        fragment: g,
        kind: "guess",
        phrase: null,
        why: `${bestCount} shared words, no shared phrase`,
      });
    }
  }

  return rankTraces(out);
}

/**
 * Folds in same-source encounters and meaning guesses without inventing a third kind.
 *
 * Same-source material that shares a run is a fact (`same words`). A meaning hit that
 * cannot show a phrase is a guess, and only if it can still say why in countable words —
 * a similarity score is never the reason. Hits with nothing shared are dropped.
 */
export function assembleTraces(
  line: Fragment[],
  corpus: Fragment[],
  rejected: ReadonlySet<string>,
  sameSource: Fragment[],
  meaning: Fragment[],
): Trace[] {
  const extra = [...sameSource, ...meaning].filter(
    (f) => !corpus.some((c) => c.id === f.id) && !line.some((m) => m.id === f.id),
  );
  const lexical = tracesFor(line, extra.length ? [...corpus, ...extra] : corpus, rejected);
  const have = new Set(lexical.map((t) => t.fragment.id));
  const lineTokens = line.map((m) => tokenize(m.body));
  const more: Trace[] = [];

  for (const g of [...sameSource, ...meaning]) {
    if (have.has(g.id) || rejected.has(g.id) || line.some((m) => m.id === g.id)) continue;
    const tg = tokenize(g.body);
    if (tg.length < MIN_TOKENS) continue;
    let bestRun: { length: number; a: string } | null = null;
    let bestCount = 0;
    for (const lt of lineTokens) {
      const run = sharedRun(tg, lt);
      if (run && (!bestRun || run.length > bestRun.length)) bestRun = run;
      bestCount = Math.max(bestCount, sharedCount(tg, lt));
    }
    if (bestRun) {
      more.push({ fragment: g, kind: "seen", phrase: bestRun.a, why: null });
      have.add(g.id);
    } else if (bestCount >= 1) {
      more.push({
        fragment: g,
        kind: "guess",
        phrase: null,
        why: `${bestCount} shared word${bestCount === 1 ? "" : "s"}, no shared phrase`,
      });
      have.add(g.id);
    }
  }

  return rankTraces([...lexical, ...more]);
}

function rankTraces(out: Trace[]): Trace[] {
  // Facts first, then guesses; newest first inside each. A guess never outranks something
  // the record can actually show you.
  out.sort((a, b) =>
    a.kind === b.kind
      ? new Date(b.fragment.capturedAt).getTime() - new Date(a.fragment.capturedAt).getTime()
      : a.kind === "seen"
        ? -1
        : 1,
  );
  return out.slice(0, MAX_TRACES);
}

/** "traces · 2 seen, 1 guessed" */
export function tracesHeading(traces: Trace[]): string {
  const seen = traces.filter((t) => t.kind === "seen").length;
  return `traces · ${seen} seen, ${traces.length - seen} guessed`;
}
