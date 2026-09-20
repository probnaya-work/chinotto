/**
 * The caret. 3px × 44px, blinking on a step(1) curve.
 *
 * It appears wherever the Record is waiting for you: above an empty record (3i), at the
 * head of capture (3b), and at the end of a Line where a continuation would land (3c).
 * It is the same object each time, which is why it lives in one place.
 */
export function Caret({ height, inline = false }: { height?: string; inline?: boolean }) {
  return (
    <span
      aria-hidden="true"
      style={{
        display: "inline-block",
        width: "var(--caret-width)",
        height: height ?? "var(--caret-height)",
        background: "var(--ink)",
        animation: `chinotto-blink var(--blink) steps(1) infinite`,
        ...(inline ? { verticalAlign: "-5px", marginLeft: "1px" } : null),
      }}
    />
  );
}

/**
 * The five bars while speaking. Each runs at its own duration in the design, so they never
 * pulse in unison and the row reads as sound rather than as a progress indicator.
 */
const REC_DURATIONS = ["0.9s", "1.1s", "0.7s", "1.3s", "0.8s"];

/**
 * `width` and `height` exist for the menu-bar panel, which draws the same meter at the
 * tray's own scale. It is the same five bars at the same five durations — one voice meter
 * in the product, as there is one caret.
 */
export function SpeakingBars({
  width = "var(--caret-width)",
  height = "var(--caret-height)",
}: {
  width?: string;
  height?: string;
} = {}) {
  return (
    <span
      aria-hidden="true"
      style={{ display: "flex", alignItems: "center", gap: "3px", height }}
    >
      {REC_DURATIONS.map((d, i) => (
        <span
          key={i}
          className="chinotto-rec-bar"
          style={{
            display: "inline-block",
            width,
            height,
            // Live: a level meter is the one place a colour is allowed, because it is the one
            // thing on the surface that is happening rather than written.
            background: "var(--live)",
            animation: `chinotto-rec ${d} ease-in-out infinite`,
          }}
        />
      ))}
    </span>
  );
}
