/**
 * The mark (3l).
 *
 * A ring with dots receding in a column — the product's own rule, drawn once. The ring is
 * the fruit and the recognisable part; it also reads as the caret's circle.
 *
 * It is drawn at three sizes and they are NOT the same artwork scaled: at 80 there are three
 * dots, at 32 two, at 16 one, and the ring's stroke thickens as the size drops so it survives
 *. Those are the design's exact values, not an interpolation.
 *
 * The design is explicit about where this may appear: "the app icon and the macOS title bar
 * only, never inside the record." Nothing in Now, the Record, Find or Fragment focus imports
 * this component, and nothing should.
 */

export type MarkSize = 80 | 32 | 16;

/** Pick the variant the design draws at or above this pixel size. */
function variantFor(px: number): MarkSize {
  if (px >= 56) return 80;
  if (px >= 24) return 32;
  return 16;
}

export function Mark({ size = 80, title }: { size?: number; title?: string }) {
  const variant = variantFor(size);

  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 64 64"
      fill="none"
      role={title ? "img" : undefined}
      aria-label={title}
      aria-hidden={title ? undefined : true}
      style={{ color: "var(--ink)" }}
    >
      {variant === 80 && (
        <>
          <circle cx="32" cy="32" r="28" stroke="currentColor" strokeWidth="2.5" />
          <circle cx="32" cy="23" r="8" fill="currentColor" />
          <circle cx="32" cy="38" r="4.5" fill="currentColor" />
          <circle cx="32" cy="47.5" r="2.5" fill="currentColor" />
        </>
      )}
      {variant === 32 && (
        <>
          <circle cx="32" cy="32" r="28" stroke="currentColor" strokeWidth="3.5" />
          <circle cx="32" cy="23" r="9" fill="currentColor" />
          <circle cx="32" cy="40" r="5" fill="currentColor" />
        </>
      )}
      {variant === 16 && (
        <>
          <circle cx="32" cy="32" r="27" stroke="currentColor" strokeWidth="6" />
          <circle cx="32" cy="27" r="11" fill="currentColor" />
        </>
      )}
    </svg>
  );
}

/**
 * The app icon tile, as 3l draws it: a rounded square carrying the 80-size mark. Used for
 * generating the bundled icon and for the About screen.
 */
export function MarkTile({ size = 80, light = false }: { size?: number; light?: boolean }) {
  return (
    <div
      style={{
        width: size,
        height: size,
        borderRadius: size * 0.225, // 18 at 80, the design's radius
        background: light ? "#f2f1ec" : "#141416",
        border: light ? "none" : "1px solid #3a3a40",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        boxSizing: "border-box",
      }}
    >
      <svg width={size * 0.625} height={size * 0.625} viewBox="0 0 64 64" fill="none">
        <g style={{ color: light ? "#1b1b1d" : "#e6e6e3" }}>
          <circle cx="32" cy="32" r="28" stroke="currentColor" strokeWidth="3" />
          <circle cx="32" cy="23" r="8" fill="currentColor" />
          <circle cx="32" cy="38" r="4.5" fill="currentColor" />
          <circle cx="32" cy="47.5" r="2.5" fill="currentColor" />
        </g>
      </svg>
    </div>
  );
}

/**
 * The wordmark. Archivo 500, lower case, and — per the design — the About screen only.
 */
export function Wordmark() {
  return (
    <span style={{ display: "inline-flex", alignItems: "center", gap: "10px" }}>
      <Mark size={16} />
      <span
        style={{
          fontSize: "17px",
          fontWeight: 500,
          color: "var(--ink-verb)",
          letterSpacing: "-0.01em",
          fontVariationSettings: "'wdth' 96",
        }}
      >
        chinotto
      </span>
      <span style={{ fontSize: "var(--size-meta)", color: "var(--meta)", marginLeft: "10px" }}>
        an instrument by PROBNAYA
      </span>
    </span>
  );
}
