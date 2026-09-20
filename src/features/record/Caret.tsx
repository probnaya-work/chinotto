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

export function SpeakingBars() {
  return (
    <span
      aria-hidden="true"
      style={{ display: "flex", alignItems: "center", gap: "3px", height: "var(--caret-height)" }}
    >
      {REC_DURATIONS.map((d, i) => (
        <span
          key={i}
          className="chinotto-rec-bar"
          style={{
            display: "inline-block",
            width: "var(--caret-width)",
            height: "var(--caret-height)",
            background: "var(--ink)",
            animation: `chinotto-rec ${d} ease-in-out infinite`,
          }}
        />
      ))}
    </span>
  );
}
