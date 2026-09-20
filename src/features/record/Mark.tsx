/**
 * The mark: a ring with three dots receding down a column.
 *
 * It is the product's own rule, drawn once. The ring is the recognisable part; the dots are
 * distance, which is the only thing this product arranges by.
 *
 * Three rungs, and they are NOT one drawing scaled. At ≥40px there are three dots, at
 * 24–39px two, at ≤20px one, and the ring's stroke thickens as the size drops so it survives
 * — a hairline ring at 16px was exactly how the old mark failed. **Picking the wrong rung is
 * the only way to get this wrong**, so the rung is chosen from the size rather than passed
 * in, and every call site gets the right one by construction.
 *
 * Where it may appear: launch, the settings header, about, the app icon, the store. Never
 * inside the record. The caret is the record's own mark and the two must not compete.
 */

export type MarkRung = "large" | "medium" | "small";

/** The rung the identity spec draws at or above a given pixel size. */
export function rungFor(px: number): MarkRung {
  if (px >= 40) return "large";
  if (px >= 24) return "medium";
  return "small";
}

export function Mark({
  size = 64,
  title,
  /** Overrides the ink, for a tile that is not on the record's surface. */
  ink,
  /**
   * The app-icon weight: the ring goes 2.5 → 3 so it holds at icon scale, where the tile's
   * own edge competes with it. Only the icon and its previews use this.
   */
  appIcon = false,
}: {
  size?: number;
  title?: string;
  ink?: string;
  appIcon?: boolean;
}) {
  const rung = rungFor(size);
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 64 64"
      fill="none"
      role={title ? "img" : undefined}
      aria-label={title}
      aria-hidden={title ? undefined : true}
      style={{ color: ink ?? "var(--ink)", flex: "none", overflow: "visible" }}
    >
      {rung === "large" && (
        <>
          <circle
            cx="32"
            cy="32"
            r="28"
            stroke="currentColor"
            strokeWidth={appIcon ? 3 : 2.5}
          />
          <circle cx="32" cy="23" r="8" fill="currentColor" />
          <circle cx="32" cy="38" r="4.5" fill="currentColor" />
          <circle cx="32" cy="47.5" r="2.5" fill="currentColor" />
        </>
      )}
      {rung === "medium" && (
        <>
          <circle cx="32" cy="32" r="28" stroke="currentColor" strokeWidth="3.5" />
          <circle cx="32" cy="23" r="9" fill="currentColor" />
          <circle cx="32" cy="40" r="5" fill="currentColor" />
        </>
      )}
      {rung === "small" && (
        <>
          <circle cx="32" cy="32" r="27" stroke="currentColor" strokeWidth="6" />
          <circle cx="32" cy="27" r="11" fill="currentColor" />
        </>
      )}
    </svg>
  );
}

/**
 * The lockup: the mark and the wordmark, and nothing else.
 *
 * Never with a tagline attached, and never below 22px of wordmark — both are rules from the
 * identity spec rather than preferences. The gap is 0.38 × the wordmark's size, so the
 * lockup holds its proportions at launch size and at about size without being re-spaced.
 */
export function Lockup({
  wordSize = 28,
  markSize,
  vertical = false,
}: {
  wordSize?: number;
  markSize?: number;
  vertical?: boolean;
}) {
  const mark = markSize ?? Math.round(wordSize * 1.07);
  return (
    <span
      style={{
        display: "inline-flex",
        flexDirection: vertical ? "column" : "row",
        alignItems: "center",
        gap: `${wordSize * 0.38}px`,
      }}
    >
      <Mark size={mark} title="Chinotto" />
      <span
        style={{
          fontSize: `${wordSize}px`,
          fontWeight: 500,
          color: "var(--ink)",
          letterSpacing: wordSize >= 52 ? "-0.025em" : "-0.01em",
          fontVariationSettings: "'wdth' 96",
          lineHeight: 1,
        }}
      >
        chinotto
      </span>
    </span>
  );
}

/**
 * The app icon as macOS draws it: the mark at 0.62 of a squircle that is itself 824 of a
 * 1024 canvas, corner radius 185. Used for the settings preview and for generating the
 * bundled rasters, so both come from one drawing.
 */
export function AppIconTile({ size = 56, light = false }: { size?: number; light?: boolean }) {
  return (
    <span
      style={{
        width: size,
        height: size,
        borderRadius: size * 0.2245, // 185 / 824
        background: light ? "#f2f1ec" : "#141416",
        border: light ? "none" : "1px solid #2a2a2e",
        boxSizing: "border-box",
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
      }}
    >
      <Mark size={Math.round(size * 0.62)} ink={light ? "#1b1b1d" : "#d4d3ce"} appIcon />
    </span>
  );
}
