/**
 * A fragment at D0 — the last eight hours.
 *
 * This is the product's basic visual object and it is deliberately not a card. Separation
 * comes from rhythm, time and material — a gutter label, a gap, a size — never from a
 * repeated frame. The only tier with a time gutter, verbs and a wash is this one, because
 * this is the only distance at which a fragment is still something you are doing rather
 * than something you have.
 *
 * The row under the pointer shows exactly three verbs (continue · hold · ⋯). Everything
 * else lives behind ⋯, which opens a row of words underneath rather than a menu on top:
 * a popover would be the loudest object on a surface whose whole argument is that material
 * is loudest.
 */

import { useEffect, useRef, type CSSProperties, type ReactNode } from "react";
import type { Encounter, Fragment, VoiceCapture } from "../../lib/recordApi";
import { metaStyle, tierStyle } from "../../design/tiers";
import { clockLabel, durationLabel, fullDateLabel, gutterLabel } from "./format";
import { Marked } from "./Marked";
import { Verb } from "./Verb";

export interface FragmentRowProps {
  fragment: Fragment;
  now: Date;
  encounter?: Encounter;
  voice?: VoiceCapture;

  /** Replaces the gutter label. "now", for a fragment just left. */
  gutterOverride?: string;
  /** Words to mark, for Find and for a Return's matched phrase. */
  mark?: string | string[] | null;
  /** "↳ moment 3 of a line · since march" — only when the fragment is in a line. */
  lineMeta?: string | null;

  active?: boolean;
  selected?: boolean;
  onHover?: (id: string | null) => void;
  onOpen?: (f: Fragment) => void;
  onContinue?: (f: Fragment) => void;
  onHold?: (f: Fragment) => void;
  held?: boolean;

  /** The inline verb row, opened by ⋯. */
  menuOpen?: boolean;
  onToggleMenu?: (f: Fragment) => void;
  onCorrect?: (f: Fragment) => void;
  onCopy?: (f: Fragment) => void;
  onCopyLink?: (f: Fragment) => void;
  onRemove?: (f: Fragment) => void;
  /** "where it came from" — a sentence, expanded in place, never a screen. */
  provenanceOpen?: boolean;
  onToggleProvenance?: (f: Fragment) => void;

  /** Paragraphs past the first, disclosed on request. */
  expanded?: boolean;
  onExpand?: (f: Fragment) => void;

  /** Correcting, in place. */
  editing?: boolean;
  editText?: string;
  onEditChange?: (v: string) => void;
  onEditSave?: () => void;
  onEditCancel?: () => void;

  /**
   * The edit window: this fragment was just left, and for a few seconds it still says so.
   * `secondsLeft` counts down; a continuation offer may ride alongside it.
   */
  justSaved?: boolean;
  /** True only for the first moment after saving, so `settle` runs once. */
  landing?: boolean;
  secondsLeft?: number;
  suggestion?: { text: string; when: string } | null;
  onAcceptSuggestion?: () => void;
  onRejectSuggestion?: () => void;

  onPlay?: (f: Fragment) => void;
  playing?: boolean;
}

export function FragmentRow(props: FragmentRowProps) {
  const {
    fragment,
    now,
    encounter,
    voice,
    gutterOverride,
    mark,
    lineMeta,
    active = false,
    selected = false,
    held = false,
    menuOpen = false,
    provenanceOpen = false,
    expanded = false,
    editing = false,
    justSaved = false,
    landing = false,
    secondsLeft = 0,
    suggestion = null,
    playing = false,
  } = props;

  const rowRef = useRef<HTMLDivElement>(null);
  const editRef = useRef<HTMLTextAreaElement>(null);
  const showVerbs = (active || selected) && !editing;

  // Keyboard selection has to stay on screen, or ↑↓ walks off the bottom invisibly.
  useEffect(() => {
    if (selected) rowRef.current?.scrollIntoView({ block: "nearest" });
  }, [selected]);

  // Correcting opens with the caret at the end of what is already there, not selecting it:
  // you are changing wording, not replacing a thought.
  useEffect(() => {
    if (!editing) return;
    const el = editRef.current;
    if (!el) return;
    el.focus();
    el.setSelectionRange(el.value.length, el.value.length);
  }, [editing]);

  useEffect(() => {
    const el = editRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight}px`;
  }, [editing, props.editText]);

  const hasOwnWords = fragment.body.trim().length > 0;
  const displayText = hasOwnWords
    ? fragment.body
    : (encounter?.title ?? encounter?.urlRaw ?? fragment.body);
  const paragraphs = displayText.split(/\n\n+/);
  const moreParagraphs = paragraphs.length - 1;

  const bodyStyle: CSSProperties = {
    ...tierStyle("d0"),
    cursor: "pointer",
    display: "inline-block",
    maxWidth: "100%",
    // A transcript and a link's own title are both in someone else's voice.
    ...(voice ? { fontStyle: "italic", color: "var(--ink-far)" } : null),
    ...(!hasOwnWords && encounter ? { color: "var(--ink-far)" } : null),
    // The fragment you just left keeps a line under it for as long as it is still yours
    // to change — the same mark correcting uses, saying the same thing.
    ...(justSaved ? { boxShadow: "inset 0 -2px var(--correction-rule)" } : null),
  };

  return (
    <div
      ref={rowRef}
      role="listitem"
      data-selected={selected || undefined}
      aria-current={selected || undefined}
      onMouseEnter={() => props.onHover?.(fragment.id)}
      onMouseLeave={() => props.onHover?.(null)}
      style={{
        display: "flex",
        gap: "var(--gutter-gap)",
        position: "relative",
        // The wash bleeds past the text on both sides, so the row reads as picked up
        // rather than boxed in. Negative margin plus matching padding keeps the row's
        // height identical hovered and not: the record must not move under the pointer.
        margin: "0 -20px",
        padding: "6px 20px",
        background: active || selected ? "var(--row-hover)" : "transparent",
        animation: landing ? "chinotto-settle var(--settle) var(--ease)" : "none",
      }}
    >
      <span
        style={{
          ...metaStyle(),
          width: "var(--gutter-width)",
          flex: "none",
          // Optically aligns the 12px label with the first line of 26px material.
          paddingTop: "9px",
        }}
      >
        {gutterOverride ?? gutterLabel(new Date(fragment.capturedAt), now)}
      </span>

      <div style={{ flex: 1, minWidth: 0 }}>
        {editing ? (
          <>
            <textarea
              ref={editRef}
              rows={1}
              value={props.editText ?? ""}
              onChange={(e) => props.onEditChange?.(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !e.shiftKey) {
                  e.preventDefault();
                  props.onEditSave?.();
                }
                if (e.key === "Escape") {
                  e.preventDefault();
                  e.stopPropagation();
                  props.onEditCancel?.();
                }
              }}
              spellCheck={false}
              aria-label="Correct the wording"
              style={{
                width: "100%",
                resize: "none",
                border: "none",
                outline: "none",
                background: "transparent",
                color: "var(--ink)",
                font: "inherit",
                fontSize: "var(--size-d0)",
                lineHeight: 1.24,
                letterSpacing: "var(--track-d0)",
                boxShadow: "inset 0 -2px var(--correction-rule)",
                paddingBottom: "4px",
                overflow: "hidden",
              }}
            />
            <div
              style={{
                ...metaStyle(),
                marginTop: "10px",
                display: "flex",
                gap: "18px",
              }}
            >
              {/* Said on the spot, because this is the one action that changes words. */}
              <span>
                changes the wording only — the moment keeps its date and the earlier wording
              </span>
              <Verb ink onClick={() => props.onEditSave?.()} style={{ marginLeft: "auto" }}>⏎ save</Verb>
              <Verb onClick={() => props.onEditCancel?.()}>esc cancel</Verb>
            </div>
          </>
        ) : (
          <>
            {encounter?.selectedText ? (
              // What the source said, above what you said about it, behind its own rule.
              <div
                onClick={() => props.onOpen?.(fragment)}
                style={{
                  color: "var(--ink-far)",
                  paddingLeft: "18px",
                  borderLeft: "2px solid var(--rule)",
                  fontSize: "var(--size-quote)",
                  lineHeight: 1.3,
                  marginBottom: "10px",
                  cursor: "pointer",
                  textWrap: "pretty",
                }}
              >
                “{encounter.selectedText}”
              </div>
            ) : null}

            <div onClick={() => props.onOpen?.(fragment)} style={bodyStyle}>
              {voice ? (
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    props.onPlay?.(fragment);
                  }}
                  disabled={voice.audioMissing}
                  title={
                    voice.audioMissing
                      ? "the audio for this recording is not on this device"
                      : undefined
                  }
                  style={{
                    display: "inline-block",
                    fontSize: "var(--size-voice-chip)",
                    color: voice.audioMissing ? "var(--meta)" : "var(--ink-verb)",
                    border: "1px solid var(--rule)",
                    borderStyle: voice.audioMissing ? "dashed" : "solid",
                    padding: "3px 10px 3px 8px",
                    verticalAlign: "4px",
                    marginRight: "12px",
                    fontStyle: "normal",
                    background: "none",
                    cursor: voice.audioMissing ? "default" : "pointer",
                    fontVariationSettings: "'wdth' 90",
                  }}
                >
                  {voice.audioMissing ? "⌀" : playing ? "■" : "▶"}{" "}
                  {durationLabel(voice.durationMs / 1000)}
                </button>
              ) : null}
              {voice && !hasOwnWords ? (
                // The recording exists; the reading of it does not, or failed. Saying which
                // is the whole point — the audio is safe either way, and a blank row would
                // imply the opposite.
                <span style={{ ...metaStyle("var(--size-voice-chip)"), fontStyle: "normal" }}>
                  {voice.transcriptState === "failed" ? (
                    <>
                      couldn’t transcribe ·{" "}
                      {voice.audioMissing
                        ? "the audio is not on this device"
                        : "the audio is safe"}{" "}
                      ·{" "}
                      <Verb onClick={() => props.onCorrect?.(fragment)}>type it</Verb>
                    </>
                  ) : (
                    "listening back…"
                  )}
                </span>
              ) : (
                <Marked text={expanded ? displayText : paragraphs[0]} mark={mark} />
              )}
            </div>

            {expanded && moreParagraphs > 0
              ? null
              : moreParagraphs > 0 && (
                  <Affordance onClick={() => props.onExpand?.(fragment)}>
                    ↓ {moreParagraphs} more paragraph{moreParagraphs === 1 ? "" : "s"}
                  </Affordance>
                )}

            {/*
              The source line. A title that has not arrived is not a missing field to
              apologise for — the address is the material until the title exists.
            */}
            {encounter ? (
              <Meta onClick={() => props.onOpen?.(fragment)}>
                {sourceLine(encounter, hasOwnWords)}
              </Meta>
            ) : null}

            {voice?.audioMissing && hasOwnWords ? (
              <Meta>the audio is no longer on this device · these words remain</Meta>
            ) : null}

            {lineMeta ? <Meta>{lineMeta}</Meta> : null}

            {justSaved ? (
              <span
                style={{
                  ...metaStyle("var(--size-meta-lg)"),
                  display: "flex",
                  gap: "26px",
                  marginTop: "8px",
                }}
              >
                <Verb onClick={() => props.onCorrect?.(fragment)}>
                  still yours to change · {secondsLeft}s
                </Verb>
                {suggestion ? (
                  <span>
                    continues {suggestion.when}{" "}
                    <span style={{ color: "var(--ink-verb)" }}>“{suggestion.text}”</span>?{" "}
                    <Verb
                      onClick={() => props.onAcceptSuggestion?.()}
                      ink
                      style={{ textDecoration: "underline", textUnderlineOffset: "4px" }}
                    >
                      yes
                    </Verb>{" "}
                    ·{" "}
                    <Verb onClick={() => props.onRejectSuggestion?.()}>no</Verb>
                  </span>
                ) : null}
              </span>
            ) : null}

            {menuOpen ? (
              <div
                style={{
                  ...metaStyle("var(--size-meta-lg)"),
                  color: "var(--ink-verb)",
                  display: "flex",
                  gap: "22px",
                  marginTop: "10px",
                }}
              >
                <Verb bright onClick={() => props.onCorrect?.(fragment)}>correct</Verb>
                <Verb bright onClick={() => props.onCopy?.(fragment)}>copy</Verb>
                {encounter ? (
                  <Verb bright onClick={() => props.onCopyLink?.(fragment)}>copy link</Verb>
                ) : null}
                <Verb bright onClick={() => props.onToggleProvenance?.(fragment)}>where it came from</Verb>
                <Verb bright onClick={() => props.onRemove?.(fragment)} style={{ marginLeft: "auto" }}>remove</Verb>
              </div>
            ) : null}

            {provenanceOpen ? <Meta>{provenanceOf(fragment, encounter, voice, now)}</Meta> : null}
          </>
        )}
      </div>

      {/*
        Always mounted, hidden until the row is active. Mounting on hover would narrow the
        body and re-wrap its text mid-gesture, which reads as the row flinching.
      */}
      <div
        aria-hidden={!showVerbs}
        style={{
          ...metaStyle("var(--size-meta-lg)"),
          color: "var(--ink-verb)",
          display: "flex",
          gap: "18px",
          flex: "none",
          paddingTop: "9px",
          visibility: showVerbs ? "visible" : "hidden",
          pointerEvents: showVerbs ? "auto" : "none",
        }}
      >
        <Verb bright onClick={() => props.onContinue?.(fragment)}>continue</Verb>
        <Verb bright onClick={() => props.onHold?.(fragment)}>{held ? "release" : "hold"}</Verb>
        <Verb bright aria-label="more" onClick={() => props.onToggleMenu?.(fragment)}>⋯</Verb>
      </div>
    </div>
  );
}

function Meta({ children, onClick }: { children: ReactNode; onClick?: () => void }) {
  return (
    <span
      onClick={onClick}
      style={{
        ...metaStyle("var(--size-meta-lg)"),
        display: "block",
        marginTop: "8px",
        cursor: onClick ? "pointer" : undefined,
      }}
    >
      {children}
    </span>
  );
}

function Affordance({ children, onClick }: { children: ReactNode; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={(e) => {
        e.stopPropagation();
        onClick();
      }}
      className="chinotto-affordance"
      style={{
        ...metaStyle("var(--size-meta-lg)"),
        display: "block",
        marginTop: "8px",
        background: "none",
        border: "none",
        padding: 0,
        textAlign: "left",
        cursor: "pointer",
      }}
    >
      {children}
    </button>
  );
}


/** "theatlantic.com · Why Everyone Suddenly Wants a Second Brain" */
function sourceLine(e: Encounter, hasOwnWords: boolean): string {
  const domain = e.domain ?? "a link";
  if (hasOwnWords) return e.title ? `${domain} · ${e.title}` : domain;
  const shared = e.sourceApp ? "shared, " : "";
  // Only claim the title is coming if it actually still might.
  const tail =
    e.enrichmentState === "pending"
      ? "title arrives when you are back online"
      : e.title
        ? e.title
        : "no words yet";
  return `${domain} · ${shared}${tail}`;
}

/**
 * One sentence saying where a fragment came from, or as much of it as is actually known.
 * Material carried over from v1 has no recorded capture method, so it says the date and
 * stops rather than asserting one.
 */
function provenanceOf(
  f: Fragment,
  e: Encounter | undefined,
  v: VoiceCapture | undefined,
  now: Date,
): string {
  const at = new Date(f.capturedAt);
  const parts: string[] = [];

  if (v) parts.push("voice");
  else if (e) parts.push(e.sourceApp ? `shared from ${e.sourceApp}` : "a link you typed");
  else if (f.captureMethod === "imported") parts.push("carried over");
  else if (f.captureOrigin === "menubar") parts.push("typed from the menu bar");
  else parts.push(f.captureMethod);

  parts.push(f.captureOrigin === "mobile" ? "your phone" : "this mac");
  parts.push(`${fullDateLabel(at)} ${clockLabel(at)}`);
  if (f.correctionCount > 0) parts.push("wording corrected");
  else if (f.legacyEditCount > 0) parts.push("edited before the record kept wordings");
  void now;
  return parts.join(" · ");
}
