/**
 * A year, folded into one row.
 *
 * This is what the floor produces: past six months material stops being readable as
 * material, so a whole year becomes a label, twelve month ticks showing where material
 * exists, a count, and a run of first lines trailing off. Clicking the year or a month
 * stands you there.
 *
 * The ticks are a density indicator and nothing more — not productivity, not mood, not a
 * summary. A month with nothing in it is drawn at the minimum rather than omitted, so the
 * shape of a year keeps its gaps, and the scale is absolute rather than per-year, so a
 * quiet year looks quiet next to a loud one.
 */

import { metaStyle } from "../../design/tiers";
import { MONTHS } from "./format";
import { yearBarHeight } from "./bands";

export interface YearRowProps {
  year: number;
  /** Twelve counts, january first. */
  density: number[];
  count: number;
  /** First lines from across the year, already ordered. */
  firstLines?: string[];
  onStandIn?: (year: number, month: number) => void;
  /** The last month that has anything, for a click on the year itself. */
  lastMonthWithMaterial?: number;
}

export function YearRow({
  year,
  density,
  count,
  firstLines = [],
  onStandIn,
  lastMonthWithMaterial,
}: YearRowProps) {
  // Clicking a year lands you at its far end, where the year was still being written,
  // rather than in a january that may hold nothing.
  const landing =
    lastMonthWithMaterial ??
    density.reduce((best, n, i) => (n > 0 ? i : best), 0);

  return (
    <div
      role="listitem"
      style={{
        display: "flex",
        alignItems: "flex-end",
        gap: "16px",
        height: "var(--year-row-height)",
      }}
    >
      <button
        type="button"
        className="chinotto-affordance"
        onClick={() => onStandIn?.(year, landing)}
        style={{
          ...metaStyle("var(--size-year-label)"),
          color: "var(--ink-far)",
          width: "var(--year-label-width)",
          textAlign: "left",
          background: "none",
          border: "none",
          padding: 0,
          cursor: "pointer",
        }}
      >
        {year}
      </button>

      <div
        style={{
          display: "flex",
          alignItems: "flex-end",
          gap: "var(--year-bar-gap)",
          height: "16px",
        }}
      >
        {density.map((n, month) => (
          <button
            key={month}
            type="button"
            title={`${MONTHS[month]} ${year} · ${n}`}
            aria-label={`${MONTHS[month]} ${year}, ${n}`}
            disabled={n === 0}
            onClick={() => onStandIn?.(year, month)}
            style={{
              display: "inline-block",
              width: "var(--year-bar-width)",
              height: `${yearBarHeight(n)}px`,
              padding: 0,
              border: "none",
              background: n > 0 ? "var(--year-bar-ink)" : "var(--year-bar-empty)",
              cursor: n > 0 ? "pointer" : "default",
            }}
          />
        ))}
      </div>

      <span style={{ ...metaStyle(), width: "28px", fontVariationSettings: "'wdth' 80" }}>
        {count}
      </span>

      {/*
        A run of what is in there, cut off wherever the column ends. Not a summary — a year
        cannot be summarised in a line, and pretending otherwise would be the one claim this
        row must not make.
      */}
      <span
        style={{
          ...metaStyle(),
          fontVariationSettings: "'wdth' 76",
          whiteSpace: "nowrap",
          overflow: "hidden",
          textOverflow: "ellipsis",
          flex: 1,
          minWidth: 0,
        }}
      >
        {firstLines.join(" · ")}
      </span>
    </div>
  );
}
