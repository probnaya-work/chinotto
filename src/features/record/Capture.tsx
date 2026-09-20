/**
 * The edge.
 *
 * One field. It is capture, and it is also find, and it is also how you go and stand in
 * march 2024 — not because those are modes it switches between, but because there is
 * nothing else to type into. `/` makes it find, a date makes it a destination, anything
 * else makes it a fragment. Nothing has to be chosen before typing, which is the whole
 * argument: capture is an action, not a form.
 *
 * Capture never waits. `⏎` clears the field before anything else happens, because the save
 * is local and cannot fail, and a field that sits there looking like it is asking
 * permission is a field that teaches you to hesitate.
 */

import { useEffect, useRef, useState, type ReactNode } from "react";
import { SpeakingBars } from "./Caret";
import { metaStyle } from "../../design/tiers";
import { anchorHint, parseAnchor, type ParsedAnchor } from "./anchors";
import { durationLabel } from "./format";
import { Verb } from "./Verb";

export interface CaptureProps {
  value: string;
  onChange: (v: string) => void;

  /** Leave it. Rejecting is not possible: capture is local and cannot fail. */
  onLeave: (body: string) => void;
  /** A date phrase, resolved. `null` means "back to today". */
  onStand: (anchor: ParsedAnchor | null) => void;

  /**
   * Held `space` on an empty field, or the mouse on the hint.
   *
   * Only the press is here. The release is listened for on the window, in `useVoice`,
   * because it does not reliably arrive at whatever took the press.
   */
  onStartSpeaking?: () => void;
  speaking?: boolean;
  speakingSeconds?: number;
  /** The live transcript, while speaking. */
  transcript?: string;

  /** Down-arrow out of the edge and into the record below. */
  onEnterRecord?: () => void;

  /** Find: how many the words matched, and the way to widen it. */
  findCount?: number;
  meaningOn?: boolean;
  onToggleMeaning?: () => void;

  /** A sentence the edge has to say — the microphone, once. */
  notice?: ReactNode;

  autoFocus?: boolean;
}

export function Capture({
  value,
  onChange,
  onLeave,
  onStand,
  onStartSpeaking,
  speaking = false,
  speakingSeconds = 0,
  transcript = "",
  onEnterRecord,
  findCount = 0,
  meaningOn = false,
  onToggleMeaning,
  notice,
  autoFocus = true,
}: CaptureProps) {
  const ref = useRef<HTMLTextAreaElement>(null);
  const [focused, setFocused] = useState(false);

  useEffect(() => {
    // preventScroll: focusing otherwise scrolls the field into view, which drags the whole
    // record back to the top every time you return to the edge.
    if (autoFocus) ref.current?.focus({ preventScroll: true });
  }, [autoFocus]);

  // The field grows downward with the text rather than scrolling inside a fixed box: long
  // material needs room, but capture never becomes a writing mode.
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight}px`;
  }, [value]);

  const isFind = value.startsWith("/");
  const hasText = value.length > 0;
  const parsed = hasText && !isFind ? parseAnchor(value) : undefined;
  const hint = anchorHint(parsed);

  /**
   * The drawn bar is the caret's resting form: it stands there when the field is empty and
   * unfocused, saying the record is waiting. The moment there is focus or text, the real
   * caret takes over and two carets would be one too many.
   */
  const showBar = !hasText && !focused;

  if (speaking) {
    return (
      <div>
        <div
          style={{
            display: "flex",
            alignItems: "center",
            minHeight: "var(--capture-height)",
            gap: "26px",
          }}
        >
          <SpeakingBars />
          <span style={{ fontSize: "var(--size-capture)", letterSpacing: "var(--track-capture)" }}>
            {durationLabel(speakingSeconds)}
          </span>
          <span style={{ ...metaStyle("var(--size-meta-lg)"), marginLeft: "auto" }}>
            release to leave it · esc to drop
          </span>
        </div>
        {/*
          What is being heard, as it is heard. In the transcript's own voice — italic, in
          the quieter ink — because these are not yet words you have written down.
        */}
        <div
          style={{
            marginTop: "14px",
            fontSize: "var(--size-d0)",
            lineHeight: 1.24,
            color: "var(--ink-far)",
            fontStyle: "italic",
            textWrap: "pretty",
            maxWidth: "820px",
            minHeight: "32px",
          }}
        >
          {transcript}
          <span style={{ color: "var(--faint)" }}> …</span>
        </div>
      </div>
    );
  }

  function handleKeyDown(e: React.KeyboardEvent<HTMLTextAreaElement>) {
    // Hold space on an empty field to speak. `repeat` guards the key's own auto-repeat,
    // which would otherwise start a new recording sixty times a second.
    if (e.key === " " && !hasText && !e.repeat) {
      e.preventDefault();
      onStartSpeaking?.();
      return;
    }
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      const body = value.trim();
      // Find is a way of looking, not a thing to leave: return does nothing in it.
      if (!body || body.startsWith("/")) return;
      const anchor = parseAnchor(body);
      if (anchor !== undefined) {
        onChange("");
        onStand(anchor);
        return;
      }
      onChange("");
      onLeave(body);
      return;
    }
    if (e.key === "ArrowDown" && !hasText) {
      // Leaving the edge is how you reach the record's own keys (c, h, ⏎).
      e.preventDefault();
      onEnterRecord?.();
    }
    // `esc` is handled once, globally, so it always steps back by exactly one level.
  }

  return (
    <div>
      <div
        style={{
          display: "flex",
          alignItems: "flex-start",
          minHeight: "var(--capture-height)",
          gap: "18px",
        }}
      >
        {showBar ? (
          <span
            onClick={() => ref.current?.focus()}
            style={{
              display: "inline-block",
              width: "var(--caret-width)",
              height: "var(--caret-height)",
              background: "var(--ink)",
              marginTop: "4px",
              flex: "none",
            }}
          />
        ) : null}
        <textarea
          ref={ref}
          rows={1}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          onKeyDown={handleKeyDown}
          onFocus={() => setFocused(true)}
          onBlur={() => setFocused(false)}
          aria-label="Leave a fragment"
          spellCheck={false}
          style={{
            flex: 1,
            width: "100%",
            resize: "none",
            border: "none",
            outline: "none",
            background: "transparent",
            color: "var(--ink)",
            font: "inherit",
            fontSize: "var(--size-capture)",
            lineHeight: 1.2,
            letterSpacing: "var(--track-capture)",
            caretColor: "var(--ink)",
            padding: 0,
            minHeight: "53px",
            overflow: "hidden",
          }}
        />

        {!hasText ? (
          <span
            onMouseDown={onStartSpeaking}
            className="chinotto-verb chinotto-verb--quiet"
            style={{
              ...metaStyle("var(--size-meta-lg)"),
              // A quiet verb rather than a hint: it is the only way holding space to speak
              // is discoverable at all, and the design inks it accordingly.
              color: "var(--agency-quiet)",
              paddingTop: "22px",
              cursor: "pointer",
              userSelect: "none",
              whiteSpace: "nowrap",
            }}
          >
            ◌ hold space to speak
          </span>
        ) : null}

        {isFind && value.slice(1).trim().length > 0 ? (
          <span
            style={{
              ...metaStyle("var(--size-meta-lg)"),
              paddingTop: "22px",
              whiteSpace: "nowrap",
            }}
          >
            {findCount} in words ·{" "}
            <Verb onClick={() => onToggleMeaning?.()}>
              {meaningOn ? "in words only" : "also by meaning"}
            </Verb>{" "}
            · esc
          </span>
        ) : null}
      </div>

      {/*
        The hint row appears only while there is something to do with it. It occupies its
        line either way, so the first keystroke does not push the whole record down.
      */}
      <div
        aria-hidden={!hasText || isFind}
        style={{
          display: "flex",
          gap: "26px",
          marginTop: "6px",
          ...metaStyle("var(--size-meta-lg)"),
          color: "var(--faint)",
          opacity: hasText && !isFind ? 1 : 0,
          transition: "opacity var(--settle) var(--ease)",
        }}
      >
        <span>⏎ leave it</span>
        <span>⇧⏎ new line</span>
        <span>esc drop it</span>
        <span style={{ marginLeft: "auto" }}>{hint}</span>
      </div>

      {notice ? (
        <div
          style={{
            display: "flex",
            gap: "var(--gutter-gap)",
            marginTop: "8px",
            fontSize: "var(--size-utility-2)",
            lineHeight: 1.4,
            color: "var(--ink-far)",
            fontVariationSettings: "'wdth' 92",
          }}
        >
          <span style={{ width: "var(--gutter-width)", flex: "none" }} />
          <div>{notice}</div>
        </div>
      ) : null}
    </div>
  );
}
