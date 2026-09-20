/**
 * Standing somewhere, by typing where.
 *
 * There is no date picker and no calendar, because the record has no navigation: you write
 * where you want to be in the same field you write everything else in, and `⏎` takes you
 * there. `march 2024`, `2019`, `today`, `now`.
 *
 * The parser is deliberately narrow. Anything it does not recognise is not a date — it is
 * a fragment, and pressing return leaves it. A loose parser here would eat people's words.
 */

import { MONTHS } from "./format";

const MONTH_FULL = [
  "january",
  "february",
  "march",
  "april",
  "may",
  "june",
  "july",
  "august",
  "september",
  "october",
  "november",
  "december",
];

export interface ParsedAnchor {
  year: number;
  /** 0-based, or -1 for a bare year, meaning "wherever that year was still being written". */
  month: number;
}

/**
 * Three outcomes, and they are not the same:
 *
 *   `undefined` — not a date. Leave it as a fragment.
 *   `null`      — "today" or "now". Go back to the edge.
 *   an anchor   — stand there.
 */
export function parseAnchor(text: string): ParsedAnchor | null | undefined {
  const s = text.trim().toLowerCase();
  if (s === "today" || s === "now") return null;

  const monthYear = s.match(/^([a-z]+)\s+(20\d\d)$/);
  if (monthYear) {
    const [, word, year] = monthYear;
    const i = MONTHS.indexOf(word.slice(0, 3));
    // "mar 2024" and "march 2024", but not "mardi 2024": a three-letter abbreviation is
    // accepted as written, anything longer has to actually prefix the month's name.
    if (i >= 0 && (MONTH_FULL[i].startsWith(word) || word.length === 3)) {
      return { year: Number(year), month: i };
    }
  }

  const bareYear = s.match(/^(20\d\d)$/);
  if (bareYear) return { year: Number(bareYear[1]), month: -1 };

  return undefined;
}

/** "⏎ stand in march 2024" / "⏎ back to today" — the hint while a date is being typed. */
export function anchorHint(parsed: ParsedAnchor | null | undefined): string {
  if (parsed === undefined) return "";
  if (parsed === null) return "⏎ back to today";
  return `⏎ stand in ${parsed.month < 0 ? parsed.year : `${MONTHS[parsed.month]} ${parsed.year}`}`;
}

/**
 * A bare year lands you at its far end — the month it was last being written in — rather
 * than in a january that may hold nothing.
 */
export function resolveAnchor(
  parsed: ParsedAnchor,
  capturedAtIsoList: readonly string[],
): ParsedAnchor {
  if (parsed.month >= 0) return parsed;
  let best = -1;
  for (const iso of capturedAtIsoList) {
    const d = new Date(iso);
    if (d.getFullYear() === parsed.year && d.getMonth() > best) best = d.getMonth();
  }
  return { year: parsed.year, month: best < 0 ? 0 : best };
}
