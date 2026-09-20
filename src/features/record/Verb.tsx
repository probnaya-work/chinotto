/**
 * A verb.
 *
 * The product's controls are words — `release`, `let go ↓`, `bring back`, `show this one
 * instead`. They must still be reachable by keyboard and announced as controls, which a
 * `<span onClick>` is not, so every one of them goes through here.
 *
 * It is a `<button>` stripped of everything a button normally brings: no background, no
 * border, no padding, no font of its own. What it keeps is the part that matters — focus,
 * Enter and Space, and a role a screen reader can name.
 */

import type { CSSProperties, ReactNode } from "react";

export interface VerbProps {
  children: ReactNode;
  onClick: () => void;
  /** An active verb, in full ink. The default is the quieter register around it. */
  ink?: boolean;
  /** For the few places that want a specific step on the ladder. */
  tone?: string;
  style?: CSSProperties;
  "aria-label"?: string;
  disabled?: boolean;
}

export function Verb({
  children,
  onClick,
  ink = false,
  tone,
  style,
  disabled = false,
  ...rest
}: VerbProps) {
  return (
    <button
      type="button"
      className="chinotto-verb"
      disabled={disabled}
      onClick={(e) => {
        // A verb inside a row must not also open the row.
        e.stopPropagation();
        onClick();
      }}
      style={{
        background: "none",
        border: "none",
        padding: 0,
        margin: 0,
        font: "inherit",
        textAlign: "inherit",
        color: tone ?? (ink ? "var(--ink)" : "inherit"),
        cursor: disabled ? "default" : "pointer",
        ...style,
      }}
      {...rest}
    >
      {children}
    </button>
  );
}
