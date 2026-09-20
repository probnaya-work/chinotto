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
  /**
   * Hovers past ink, to white.
   *
   * The design spends this on a row's own verbs and on the ⋯ menu that opens from them —
   * the two places where a control sits on top of material rather than beside it.
   */
  bright?: boolean;
  /**
   * Hovers to the verb register rather than to ink.
   *
   * For words that are always on screen — the quiet line, "wording corrected" — and should
   * not flare when the pointer only crosses them.
   */
  quiet?: boolean;
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
  bright = false,
  quiet = false,
  tone,
  style,
  disabled = false,
  ...rest
}: VerbProps) {
  return (
    <button
      type="button"
      className={
        "chinotto-verb" +
        (bright ? " chinotto-verb--bright" : "") +
        (quiet ? " chinotto-verb--quiet" : "")
      }
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
