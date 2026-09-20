/**
 * "continues yesterday's '…'? yes · no"
 *
 * The offer that appears beside a fresh capture when it looks like more of something
 * recent. It is an offer and never an action: accepting joins the two into a line,
 * declining is silent, and ignoring it is the same as declining — it goes when the edit
 * window closes.
 *
 * The window it looks back over is three days. Further than that and "continues" stops
 * being a claim about what you were just doing and becomes a claim about what you think,
 * which is not this feature's business.
 */

import type { Fragment } from "../../lib/recordApi";
import { sharedCount, sharedRun, tokenize } from "./words";
import { dayLabel, firstLineOf } from "./format";

const DAY = 86_400_000;
const LOOKBACK_DAYS = 3;

/**
 * Below this, the two simply share ordinary language. The score weights a shared *run*
 * twice as heavily as scattered shared words, because a run is evidence of continuing a
 * sentence and scattered words are evidence of speaking English.
 */
const MIN_SCORE = 2;

export interface ContinuationOffer {
  id: string;
  /** The first line of what it might continue, for the offer's own words. */
  text: string;
  /** "yesterday's", "today's", "tuesday's" — possessive, as the offer reads it. */
  when: string;
}

export function suggestContinuation(
  fresh: Fragment,
  candidates: Fragment[],
  now: Date,
): ContinuationOffer | null {
  const tf = tokenize(fresh.body);
  // One or two words cannot continue anything; they can only coincide.
  if (tf.length < 2) return null;

  const nowMs = now.getTime();
  let best: { id: string; score: number; at: Date } | null = null;

  for (const g of candidates) {
    if (g.id === fresh.id) continue;
    const at = new Date(g.capturedAt);
    if (nowMs - at.getTime() > LOOKBACK_DAYS * DAY) continue;
    if (!g.body.trim()) continue;

    const tg = tokenize(g.body);
    const run = sharedRun(tf, tg);
    const score = (run ? run.length * 2 : 0) + sharedCount(tf, tg);
    if (score >= MIN_SCORE && (!best || score > best.score)) {
      best = { id: g.id, score, at };
    }
  }

  if (!best) return null;
  const target = candidates.find((f) => f.id === best!.id);
  if (!target) return null;

  return {
    id: best.id,
    text: firstLineOf(target.body),
    when: possessive(dayLabel(best.at, now)),
  };
}

/** "today" → "today's"; "wed 17" → "wed 17's". The offer is written as a possessive. */
function possessive(label: string): string {
  return `${label}'s`;
}
