/**
 * A fragment at D1–D4: one line of type, and nothing else.
 *
 * Everything that makes a D0 row a row — the time gutter, the hover verbs, the wash, the
 * provenance line — is absent here, and that absence is the design. Past the last few hours
 * a fragment is a trace you scan, not an object you act on; giving it affordances would
 * make the whole column a list again. Acting on it means opening it, which is the one thing
 * a click does.
 *
 * The material still says what kind it is: a recording keeps its chip (inert at this size —
 * there is no room to play something you cannot read), a quotation keeps its words in meta
 * ahead of your own, a link keeps its domain behind them.
 */

import type { CSSProperties } from "react";
import type { Encounter, Fragment, VoiceCapture } from "../../lib/recordApi";
import { tierStyle, type Tier } from "../../design/tiers";
import { durationLabel } from "./format";
import { Marked } from "./Marked";

export interface CompactRowProps {
  fragment: Fragment;
  tier: Exclude<Tier, "d0">;
  onOpen?: (f: Fragment) => void;
  /** Words to mark, for Find. Exact matches only — a guess is never marked. */
  mark?: string;
  encounter?: Encounter;
  voice?: VoiceCapture;
}

export function CompactRow({ fragment, tier, onOpen, mark, encounter, voice }: CompactRowProps) {
  const style: CSSProperties = {
    ...tierStyle(tier),
    cursor: "pointer",
    ...(voice ? { fontStyle: "italic" } : null),
  };

  // A link with no words of its own reads as its title — or, until the title arrives, as
  // the address. Neither is something you wrote, so neither pretends to be.
  const hasOwnWords = fragment.body.trim().length > 0;
  const text = hasOwnWords
    ? fragment.body
    : (encounter?.title ?? encounter?.urlRaw ?? fragment.body);

  return (
    <div
      role="listitem"
      onClick={() => onOpen?.(fragment)}
      className="chinotto-compact-row"
      style={style}
    >
      {voice ? (
        <span
          aria-hidden="true"
          style={{
            display: "inline-block",
            fontSize: "var(--size-voice-chip-sm)",
            // Inert: the compact tiers draw the chip but do not play from it, so it takes
            // the quiet rank and the dimmer of the two chip borders.
            color: "var(--agency-quiet)",
            fontWeight: "var(--agency-weight)",
            border: "1px solid var(--chip-border-inert)",
            padding: "1px 7px 1px 5px",
            verticalAlign: "2px",
            marginRight: "8px",
            fontStyle: "normal",
            fontVariationSettings: "'wdth' 90",
          }}
        >
          ▶ {durationLabel(voice.durationMs / 1000)}
        </span>
      ) : null}
      {encounter?.selectedText ? (
        <span style={{ color: "var(--meta)" }}>“{encounter.selectedText}” </span>
      ) : null}
      {voice && !hasOwnWords ? (
        <span style={{ fontStyle: "normal", color: "var(--meta)" }}>
          {voice.transcriptState === "failed" ? "not transcribed" : "listening back…"}
        </span>
      ) : (
        <Marked text={text.replace(/\n+/g, " ")} mark={mark} />
      )}
      {encounter?.domain && hasOwnWords ? (
        <span style={{ color: "var(--meta)" }}> {encounter.domain}</span>
      ) : null}
    </div>
  );
}
