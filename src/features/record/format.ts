/**
 * How time is written in the Record.
 *
 * Distance decides the label as much as it decides the type: a clock time inside the last
 * few hours, a weekday inside the week, a month further back, a bare year at the fold. The
 * label is chosen by where the material sits, never by a general-purpose date formatter, so
 * the gutter stays narrow and the bands stay legible at 9px.
 *
 * These are the prototype's own helpers, transcribed. They take `now` explicitly so every
 * caller and every test shares one clock.
 */

import type { Tier } from "../../design/tiers";

const MONTHS = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
const WEEKDAYS = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];
const DAY = 86_400_000;

export { MONTHS, WEEKDAYS };

/** "17:02" */
export function clockLabel(d: Date): string {
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

/** "19 sep" */
export function dayMonthLabel(d: Date): string {
  return `${d.getDate()} ${MONTHS[d.getMonth()]}`;
}

/** "19 sep 2026" */
export function fullDateLabel(d: Date): string {
  return `${d.getDate()} ${MONTHS[d.getMonth()]} ${d.getFullYear()}`;
}

/** "0:12" — a recording's length. */
export function durationLabel(seconds: number): string {
  return `${Math.floor(seconds / 60)}:${String(Math.floor(seconds % 60)).padStart(2, "0")}`;
}

export function isSameDay(a: Date, b: Date): boolean {
  return (
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate()
  );
}

/**
 * "sep" inside this year, "sep 2024" outside it.
 *
 * `forceYear` is for the places where the year is the point — standing in a month, and a
 * line's span — and where dropping it would make two different months read the same.
 */
export function monthLabel(d: Date, now: Date, forceYear = false): string {
  return !forceYear && d.getFullYear() === now.getFullYear()
    ? MONTHS[d.getMonth()]
    : `${MONTHS[d.getMonth()]} ${d.getFullYear()}`;
}

/** "today" · "yesterday" · "fri 19" · "19 sep" · "19 sep 2024", nearest form that locates it. */
export function dayLabel(d: Date, now: Date): string {
  if (isSameDay(d, now)) return "today";
  if (isSameDay(d, new Date(now.getTime() - DAY))) return "yesterday";
  if (now.getTime() - d.getTime() < 7 * DAY) return `${WEEKDAYS[d.getDay()]} ${d.getDate()}`;
  if (d.getFullYear() === now.getFullYear()) return dayMonthLabel(d);
  return fullDateLabel(d);
}

/** "this month" · "7 months" · "2 years 6 months" — how far back a Return reached. */
export function agoLabel(d: Date, now: Date): string {
  const months = Math.round((now.getTime() - d.getTime()) / (30.4 * DAY));
  if (months < 1) return "this month";
  if (months < 12) return `${months} months`;
  const years = Math.floor(months / 12);
  const rest = months % 12;
  return `${years} year${years > 1 ? "s" : ""}${rest ? ` ${rest} months` : ""}`;
}

/**
 * The gutter label for a fragment.
 *
 * Keyed on the actual date, not the tier: standing inside March 2021 renders that month at
 * D0 size, and a clock time there would locate nothing. Inside today the clock is what
 * locates a moment; anywhere else the day is.
 */
export function gutterLabel(capturedAt: Date, now: Date = new Date()): string {
  return isSameDay(capturedAt, now) ? clockLabel(capturedAt) : dayMonthLabel(capturedAt);
}

/**
 * "11s", "4m", "2h" — how long a just-left fragment has been sitting there.
 * Used only by the confirmation line under a fresh capture.
 */
export function sinceLabel(from: Date, now: Date): string {
  const s = Math.max(0, Math.round((now.getTime() - from.getTime()) / 1000));
  if (s < 60) return `${s}s`;
  const m = Math.round(s / 60);
  if (m < 60) return `${m}m`;
  return `${Math.round(m / 60)}h`;
}

/** "held here from wed · release" */
export function heldSinceLabel(heldAt: Date, now: Date): string {
  const days = (now.getTime() - heldAt.getTime()) / DAY;
  if (days < 1) return "held here";
  if (days <= 6) return `held here from ${WEEKDAYS[heldAt.getDay()]}`;
  return `held here from ${dayMonthLabel(heldAt)}`;
}

/** A URL's domain, for the source line under shared material. */
export function domainOf(url: string): string | null {
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return null;
  }
}

/**
 * The first line of a fragment, for a year row's run and for the undo notice.
 * Capped at 90 characters: it is a trace of what is in there, not a preview.
 */
export function firstLineOf(body: string): string {
  return body.split("\n")[0].slice(0, 90);
}

/** Unused by the record itself; kept for Find's own grouping. */
export function tierLabel(tier: Tier): string {
  return tier === "d1" ? "earlier today" : tier === "d2" ? "yesterday" : "earlier";
}
