/**
 * Shared language between two fragments.
 *
 * This is the prototype's word model, transcribed: a light stemmer, a stoplist, the longest
 * run of shared stems, and a count of distinct shared stems. It decides two things — whether
 * a fresh capture looks like a continuation of something recent, and which phrase to
 * highlight when two fragments are shown together.
 *
 * It is deliberately not the embedding model. Embeddings answer "close in meaning", which
 * the product always labels as a guess; this answers "the same words", which the product is
 * allowed to state as a fact. Keeping them apart is what lets a trace say `same words`
 * without hedging and `a guess` without pretending.
 *
 * It runs on material already in memory. Nothing here goes to the database, and nothing
 * here is allowed to delay a capture.
 */

const STOP = new Set(
  (
    "the a an and or but of to in on at for with is are was were be been it its this that " +
    "these those i me my we you your he she they them his her our their as by from not no " +
    "so if then than too very just about into over under again there here what which who " +
    "whom when where why how do does did done have has had having um uh ok okay i'm i've " +
    "it's that's don't"
  ).split(" "),
);

export interface Token {
  /** The word as written. */
  w: string;
  /** Its stem, which is what actually gets compared. */
  s: string;
}

function stem(word: string): string {
  let s = word.replace(/’/g, "'").replace(/'s$/, "");
  if (s.length > 5) s = s.replace(/ing$/, "");
  s = s.replace(/(edly|ed|es|ly)$/, "");
  if (s.length > 3) s = s.replace(/s$/, "");
  if (s.length > 4) s = s.replace(/e$/, "");
  return s;
}

/**
 * Words, stemmed. Capped at 160 — a long fragment's opening is enough to recognise it by,
 * and the run matcher below is quadratic.
 */
export function tokenize(text: string): Token[] {
  const words = String(text ?? "")
    .toLowerCase()
    .match(/[a-z0-9’']+/g);
  return (words ?? []).slice(0, 160).map((w) => ({ w, s: stem(w) }));
}

export interface SharedRun {
  length: number;
  /** The run as it appears in the first text. */
  a: string;
  /** The same run as it appears in the second. */
  b: string;
}

/**
 * The longest run of consecutive shared stems.
 *
 * A run only counts if it carries at least two words that are neither stoplist nor
 * two letters long, or if it is four words or more. Without that, "it is the" matches
 * everything and every fragment looks related to every other one.
 */
export function sharedRun(ta: Token[], tb: Token[]): SharedRun | null {
  let best: SharedRun | null = null;
  const m = tb.length;
  let prev = new Array<number>(m + 1).fill(0);

  for (let i = 1; i <= ta.length; i++) {
    const cur = new Array<number>(m + 1).fill(0);
    for (let j = 1; j <= m; j++) {
      if (ta[i - 1].s === tb[j - 1].s && ta[i - 1].s.length > 1) {
        cur[j] = prev[j - 1] + 1;
        if (cur[j] >= 2 && (!best || cur[j] > best.length)) {
          const A = ta.slice(i - cur[j], i);
          const B = tb.slice(j - cur[j], j);
          const carrying = A.filter((t) => !STOP.has(t.w) && t.w.length > 2).length;
          if (carrying >= 2 || cur[j] >= 4) {
            best = {
              length: cur[j],
              a: A.map((t) => t.w).join(" "),
              b: B.map((t) => t.w).join(" "),
            };
          }
        }
      }
    }
    prev = cur;
  }
  return best;
}

/** How many distinct carrying stems the two have in common, in no particular order. */
export function sharedCount(ta: Token[], tb: Token[]): number {
  const a = new Set(ta.filter((t) => !STOP.has(t.w) && t.w.length > 2).map((t) => t.s));
  const seen = new Set<string>();
  let n = 0;
  for (const t of tb) {
    if (!STOP.has(t.w) && a.has(t.s) && !seen.has(t.s)) {
      seen.add(t.s);
      n++;
    }
  }
  return n;
}

/**
 * Cuts a snippet around a phrase, on word boundaries, with ellipses where it was cut.
 * Used wherever a fragment is quoted as evidence rather than shown as itself.
 */
export function snippetAround(text: string, phrase: string): string {
  const pattern = phrase
    .trim()
    .split(/\s+/)
    .map((w) => w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"))
    .join("[\\s\\W]+");
  let match: RegExpMatchArray | null = null;
  try {
    match = text.match(new RegExp(pattern, "i"));
  } catch {
    match = null;
  }
  if (!match || match.index === undefined) return text.slice(0, 120);

  let start = Math.max(0, match.index - 28);
  let end = Math.min(text.length, match.index + match[0].length + 44);
  if (start > 0) start = text.indexOf(" ", start) + 1;
  if (end < text.length) end = text.lastIndexOf(" ", end);
  return `${start > 0 ? "…" : ""}${text.slice(start, end)}${end < text.length ? "…" : ""}`;
}
