/**
 * The quiet line. Fixed to the bottom of the window; it only says something when there is
 * something to know. Pointer events are off except on its own words, so the record can be
 * reached through it.
 *
 * Left, when present: undo, a sync notice, an update. Right: sync-on, settings.
 */

import { metaStyle } from "../../design/tiers";
import { Verb } from "./Verb";

export interface QuietLineProps {
  undo: {
    text: string;
    secondsLeft: number;
    onBringBack: () => void;
  } | null;
  /** A short, self-clearing notice (held-at-capacity, and similar). */
  notice: string | null;
  offline: boolean;
  /** Only when sync is actually configured and running, never invented. */
  syncOn: boolean;
  update: { text: string; onClick: () => void } | null;
  /**
   * A utility surface is open.
   *
   * Sync's own state goes quiet while you are looking at sync — repeating it under the
   * surface that explains it is the line talking over itself. Undo and the update still
   * speak, because both are about something that happened elsewhere and is still true.
   */
  surfaceOpen?: boolean;
  onSettings: () => void;
}

export function QuietLine({
  undo,
  notice,
  offline,
  syncOn,
  update,
  surfaceOpen = false,
  onSettings,
}: QuietLineProps) {
  return (
    <div
      style={{
        position: "fixed",
        left: "var(--window-pad-left)",
        right: "var(--window-pad-right)",
        bottom: "var(--quiet-bottom)",
        display: "flex",
        gap: "var(--quiet-gap)",
        alignItems: "baseline",
        ...metaStyle(),
        color: "var(--faint)",
        zIndex: 10,
        pointerEvents: "none",
      }}
    >
      {undo ? (
        <span style={{ color: "var(--ink-verb)", pointerEvents: "auto" }}>
          removed “{undo.text}” ·{" "}
          <Verb
            
            onClick={undo.onBringBack}
            style={{ color: "var(--ink)", cursor: "pointer" }}
          >
            bring back
          </Verb>{" "}
          · ⌘z · {undo.secondsLeft}s
        </span>
      ) : null}

      {notice && !undo && !surfaceOpen ? (
        <span style={{ color: "var(--ink-verb)", pointerEvents: "auto" }}>{notice}</span>
      ) : null}

      {offline && !undo && !surfaceOpen ? (
        <span style={{ color: "var(--ink-far)", pointerEvents: "auto" }}>offline</span>
      ) : null}

      {update && !undo ? (
        <Verb
          
          onClick={update.onClick}
          style={{ color: "var(--ink-far)", pointerEvents: "auto", cursor: "pointer" }}
        >
          {update.text}
        </Verb>
      ) : null}

      <span style={{ marginLeft: "auto", display: "flex", gap: "var(--quiet-gap)", pointerEvents: "auto" }}>
        {syncOn && !surfaceOpen ? (
          <span style={{ cursor: "default" }}>● sync on</span>
        ) : null}
        <Verb quiet onClick={onSettings} style={{ cursor: "pointer" }}>
          settings ⌘,
        </Verb>
      </span>
    </div>
  );
}
