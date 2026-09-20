/**
 * The quiet line. Fixed to the bottom of the window; it only says something when there is
 * something to know. Pointer events are off except on its own words, so the record can be
 * reached through it.
 *
 * Left, when present: undo, a sync notice, an update. Right: sync-on, settings.
 */

import { metaStyle } from "../../design/tiers";

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
  onSettings: () => void;
}

export function QuietLine({
  undo,
  notice,
  offline,
  syncOn,
  update,
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
          <span
            className="chinotto-verb"
            onClick={undo.onBringBack}
            style={{ color: "var(--ink)", cursor: "pointer" }}
          >
            bring back
          </span>{" "}
          · ⌘z · {undo.secondsLeft}s
        </span>
      ) : null}

      {notice && !undo ? (
        <span style={{ color: "var(--ink-verb)", pointerEvents: "auto" }}>{notice}</span>
      ) : null}

      {offline && !undo ? (
        <span style={{ color: "var(--ink-far)", pointerEvents: "auto" }}>offline</span>
      ) : null}

      {update && !undo ? (
        <span
          className="chinotto-verb"
          onClick={update.onClick}
          style={{ color: "var(--ink-far)", pointerEvents: "auto", cursor: "pointer" }}
        >
          {update.text}
        </span>
      ) : null}

      <span style={{ marginLeft: "auto", display: "flex", gap: "var(--quiet-gap)", pointerEvents: "auto" }}>
        {syncOn ? (
          <span style={{ cursor: "default" }}>● sync on</span>
        ) : null}
        <span className="chinotto-verb" onClick={onSettings} style={{ cursor: "pointer" }}>
          settings ⌘,
        </span>
      </span>
    </div>
  );
}
