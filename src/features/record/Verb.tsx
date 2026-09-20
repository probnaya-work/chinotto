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
 *
 * A verb is coloured at rest, which is what separates it from an ambient word that merely
 * happens to be clickable (those go through `.chinotto-affordance` and lift only on hover).
 * `font-weight` comes from `.chinotto-verb` in tokens.css rather than from here, because
 * lightness and weight together are what mark agency — neither alone is enough at 12px, and
 * 12px is where most of these are drawn.
 */

import type { CSSProperties, ReactNode } from "react";

export interface VerbProps {
  children: ReactNode;
  onClick: () => void;
  /**
   * The quiet rank: a step back (`‹ back to the edge`), a dismissal (`not this`, `let go ↓`),
   * key notation, and every destructive verb in the product (`remove`, `stop syncing on this
   * mac`, `delete the cloud account ›`).
   *
   * Rests at `--agency-quiet` and lifts to `--agency`, without an underline. Destruction is
   * never loud: its safe counterpart — `keep it` — is the one that takes full agency.
   */
  quiet?: boolean;
  /**
   * Ambient: not a verb, but clickable — `no`, a year label, `↳ first moment of a line`.
   *
   * It rests at whatever tone surrounds it and takes agency's colour and weight only under
   * the pointer, then stops being a verb again. Still a `<button>`, because it is still
   * something you can reach with a keyboard.
   */
  ambient?: boolean;
  /**
   * For the few places that want a specific step on the ladder rather than either rank —
   * a voice chip's label, or a word that is quoted rather than pressed.
   */
  tone?: string;
  style?: CSSProperties;
  "aria-label"?: string;
  disabled?: boolean;
}

export function Verb({
  children,
  onClick,
  quiet = false,
  ambient = false,
  tone,
  style,
  disabled = false,
  ...rest
}: VerbProps) {
  return (
    <button
      type="button"
      className={
        ambient ? "chinotto-affordance" : "chinotto-verb" + (quiet ? " chinotto-verb--quiet" : "")
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
        color: tone ?? (ambient ? "inherit" : quiet ? "var(--agency-quiet)" : "var(--agency)"),
        cursor: disabled ? "default" : "pointer",
        ...style,
      }}
      {...rest}
    >
      {children}
    </button>
  );
}
