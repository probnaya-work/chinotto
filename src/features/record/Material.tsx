/**
 * How external material and voice appear inside a fragment (3a, 3j).
 *
 * Both follow the same rule: show what is actually known, and say plainly when something
 * is not. A title that has not arrived is not a missing field to apologise for — the URL
 * is the material until the title exists, and then the title is.
 *
 * What is never done here: substituting a domain for a title, inferring a source app,
 * hiding a failed transcript, or implying the audio is gone when it is only untranscribed.
 */

import type { Encounter, Fragment, VoiceCapture } from "../../lib/recordApi";
import { metaStyle } from "../../design/tiers";

/** "0:06", "1:58", "31:07" */
export function duration(ms: number): string {
  const total = Math.max(0, Math.round(ms / 1000));
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${m}:${String(s).padStart(2, "0")}`;
}

/**
 * The play chip. Bordered rather than filled, sitting on the text baseline, so a recording
 * reads as part of the line rather than as an attachment bolted onto it.
 */
export function VoiceChip({
  ms,
  missing = false,
  small = false,
  onPlay,
}: {
  ms: number;
  missing?: boolean;
  small?: boolean;
  onPlay?: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onPlay}
      disabled={missing}
      title={missing ? "the audio for this recording is not on this device" : undefined}
      style={{
        display: "inline-block",
        fontSize: small ? "var(--size-meta)" : "var(--size-voice-chip)",
        color: missing ? "var(--meta)" : "var(--ink-verb)",
        border: "1px solid var(--rule)",
        padding: small ? "2px 8px 2px 6px" : "3px 10px 3px 8px",
        verticalAlign: small ? "3px" : "4px",
        marginRight: small ? "8px" : "12px",
        fontVariationSettings: "'wdth' 90",
        background: "none",
        cursor: missing ? "default" : "pointer",
        // A dashed edge says the recording is referenced but not here.
        borderStyle: missing ? "dashed" : "solid",
      }}
    >
      {missing ? "⌀" : "▶"} {duration(ms)}
    </button>
  );
}

/**
 * The source line under shared material: what the page is called, and where it came from.
 * Every part of it is optional because every part of it is fetched.
 */
export function SourceLine({
  encounter,
  hasWords,
}: {
  encounter: Encounter;
  hasWords: boolean;
}) {
  const { domain, title, enrichmentState, sourceApp, timesMet } = encounter;

  const parts: string[] = [];
  if (domain) parts.push(domain);
  if (title) parts.push(title);

  // Said once, on the fragment it concerns, in meta. Not an alert.
  if (!title) {
    if (enrichmentState === "pending") parts.push("title arrives when you're back online");
    else if (enrichmentState === "failed") parts.push("no title — the link is safe");
  }
  if (!hasWords) parts.push("shared, no words yet");
  // Only when the platform actually told us.
  if (sourceApp) parts.push(`via ${friendlyApp(sourceApp)}`);
  if (timesMet > 1) parts.push(`met ${timesMet} times`);

  if (parts.length === 0) return null;
  return (
    <span style={{ ...metaStyle("var(--size-voice-chip)"), display: "block", marginTop: "5px" }}>
      {parts.join(" · ")}
    </span>
  );
}

/**
 * Bundle identifiers are what the OS hands over; they are also unreadable. Trimming a known
 * prefix is presentation, not invention — the stored value is never changed, and anything
 * unrecognised is shown exactly as received rather than guessed at.
 */
function friendlyApp(bundleId: string): string {
  const known: Record<string, string> = {
    "com.apple.Safari": "Safari",
    "com.apple.mobilesafari": "Safari",
    "com.google.Chrome": "Chrome",
    "com.apple.MobileSMS": "Messages",
    "com.apple.mail": "Mail",
    "net.whatsapp.WhatsApp": "WhatsApp",
    "com.tinyspeck.chatlyio": "Slack",
    "com.hnc.Discord": "Discord",
  };
  return known[bundleId] ?? bundleId;
}

/**
 * The body of a fragment that carries external material.
 *
 * Order matters: the person's own words come first when they exist, because they are the
 * material. The source sits underneath. When there are no words, the title is the material,
 * and when there is no title either, the URL is — which is exactly what 3j draws.
 */
export function EncounterBody({
  fragment,
  encounter,
}: {
  fragment: Fragment;
  encounter: Encounter;
}) {
  const words = fragment.body.trim();
  const headline = words || encounter.title || encounter.urlRaw;
  const isUrlHeadline = !words && !encounter.title;

  return (
    <>
      <span
        style={{
          // A naked URL has no spaces to break at, so it is allowed to break anywhere
          // rather than widening the column.
          ...(isUrlHeadline ? { wordBreak: "break-all" } : null),
          color: words ? "var(--ink)" : "var(--ink-far)",
        }}
      >
        {headline}
      </span>
      <SourceLine encounter={encounter} hasWords={words.length > 0} />
      {encounter.selectedText && (
        /*
          What the source said, kept visibly distinct from what the person said: indented
          behind a rule, the same treatment a quotation gets in 3a.
        */
        <span
          style={{
            display: "block",
            marginTop: "10px",
            paddingLeft: "18px",
            borderLeft: "2px solid var(--rule)",
            color: "var(--ink-far)",
            fontSize: "var(--size-quote)",
            lineHeight: 1.3,
          }}
        >
          {encounter.selectedText}
        </span>
      )}
    </>
  );
}

/**
 * The body of a voice fragment.
 *
 * The transcript is italic because it is derived — a machine's reading of the audio, not
 * something typed. Once a person corrects it, it stops being italic: those are their words
 * now. A failed transcription says so, says the audio is safe, and offers both ways out.
 */
export function VoiceBody({
  fragment,
  voice,
  onPlay,
  onRetry,
  onTypeIt,
}: {
  fragment: Fragment;
  voice: VoiceCapture;
  onPlay?: () => void;
  onRetry?: () => void;
  onTypeIt?: () => void;
}) {
  const chip = <VoiceChip ms={voice.durationMs} missing={voice.audioMissing} onPlay={onPlay} />;

  if (voice.transcriptState === "failed") {
    return (
      <>
        {chip}
        <span style={metaStyle("var(--size-voice-chip)")}>
          couldn't transcribe ·{" "}
          {voice.audioMissing ? "the audio is not on this device" : "the audio is safe"} ·{" "}
          {!voice.audioMissing && (
            <>
              <Quiet onClick={onRetry}>try again</Quiet> ·{" "}
            </>
          )}
          <Quiet onClick={onTypeIt}>type it</Quiet>
        </span>
      </>
    );
  }

  if (voice.transcriptState === "pending" && !fragment.body.trim()) {
    return (
      <>
        {chip}
        <span style={metaStyle("var(--size-voice-chip)")}>listening back…</span>
      </>
    );
  }

  return (
    <>
      {chip}
      <span
        style={{
          color: voice.transcriptCorrected ? "var(--ink)" : "var(--ink-far)",
          // Italic while it is the machine's reading; upright once a person has taken it on.
          fontStyle: voice.transcriptCorrected ? "normal" : "italic",
        }}
      >
        {fragment.body}
      </span>
      {voice.audioMissing && (
        <span style={{ ...metaStyle("var(--size-voice-chip)"), display: "block", marginTop: "5px" }}>
          the audio is no longer on this device · these words remain
        </span>
      )}
    </>
  );
}

function Quiet({ children, onClick }: { children: React.ReactNode; onClick?: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      style={{
        background: "none",
        border: "none",
        padding: 0,
        font: "inherit",
        color: "var(--ink-verb)",
        cursor: "pointer",
      }}
    >
      {children}
    </button>
  );
}
