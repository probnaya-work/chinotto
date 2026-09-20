/**
 * Capture from the menu bar.
 *
 * The identity file draws this surface directly: a 440pt panel under the glyph, one
 * hairline and a deep shadow, the edge's own caret and field at the panel's scale, a hint
 * row of three, the waveform while you speak, and a Return above the caret when one is
 * waiting. It is the app's edge moved to where the cursor already is — never a list, never
 * a search field, never a settings tree. Anything that wants more room opens the window.
 *
 * The same promise as the edge: the save is local and instant, the field clears before
 * anything else happens, and nothing about sync, network or permission can be in the way.
 *
 * Nothing here is a second model of anything. Typing goes through `capture_fragment` with
 * `menubar` as its origin, speaking holds the same `useVoice` the edge holds with that
 * origin carried into `capture_voice`, and the Return is the one `select_return` chooses —
 * continued or let go against the same row the window would have used.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { emit, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import * as api from "@/lib/recordApi";
import { isFirebaseSyncConfigured } from "@/lib/firebaseConfig";
import { applyAppearance, readAppearance, readLiftContrast } from "@/lib/appearance";
import { applyStoredUiZoom } from "@/lib/uiZoom";
import { metaStyle } from "../../design/tiers";
import { SpeakingBars } from "./Caret";
import { Marked } from "./Marked";
import { Verb } from "./Verb";
import { becauseSentence, phraseOn } from "./ReturnBlock";
import { agoLabel, durationLabel, fullDateLabel } from "./format";
import { useVoice } from "./useVoice";
import { useNow } from "./useNow";
import "../../design/tokens.css";

/**
 * After `show` / `set_focus`, macOS can deliver a spurious blur before focus settles.
 * Hiding on the first blur closes the panel the instant it opens.
 */
const BLUR_HIDE_MS = 280;

/** Long enough to read "left.", short enough that it never feels like waiting. */
const CLOSE_AFTER_SAVE_MS = 520;

/**
 * Where the field stops growing and starts scrolling. The panel is the edge, not a writing
 * surface: past this the words belong in the window.
 */
const FIELD_MAX_HEIGHT = 200;

export function TrayCapture() {
  const [text, setText] = useState("");
  const [note, setNote] = useState<string | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  /** The one Return, exactly as the Record selects it. Null is the ordinary answer. */
  const [returned, setReturned] = useState<api.ReturnValue | null>(null);
  /** `continue` was pressed: what lands next is a moment on that Return's line. */
  const [continuing, setContinuing] = useState(false);

  const fieldRef = useRef<HTMLTextAreaElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const hideTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const closeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  /**
   * The webview's own zoom. The panel measures itself in CSS pixels, which browser zoom
   * does not touch, so the window's logical size is that measurement times this.
   */
  const zoom = useRef(1);

  const returnedRef = useRef(returned);
  returnedRef.current = returned;
  const continuingRef = useRef(continuing);
  continuingRef.current = continuing;

  const hide = useCallback(() => {
    void getCurrentWindow().hide();
  }, []);

  /**
   * The menu bar states `today · n` and wears its waiting modifier whether or not any
   * window is open, so every write says so. Whether sync is configured is a build-time
   * frontend fact and is carried across rather than guessed at in Rust.
   */
  const carry = useCallback(() => {
    void api.refreshTray(isFirebaseSyncConfigured()).catch(() => {});
  }, []);

  // ---- the Return -----------------------------------------------------------------------

  /**
   * Read as the panel opens, and only then.
   *
   * `select_return` is the Record's own choice and the only one: if a Return is already
   * waiting it hands back that same row, and otherwise it may surface one — which is what
   * arriving at the edge means, and arriving here is arriving at the edge. The glyph's
   * modifier is deliberately *not* this call: it asks `return_waiting`, which only reports.
   */
  const readReturn = useCallback(() => {
    void api
      .selectReturn()
      .then((r) => {
        setReturned(r);
        setContinuing(false);
        carry();
      })
      .catch(() => {});
  }, [carry]);

  // ---- speaking -------------------------------------------------------------------------

  const afterCapture = useCallback(() => {
    void emit("chinotto-tray-entry-saved", {});
    carry();
    setNote("left.");
    if (closeTimer.current) clearTimeout(closeTimer.current);
    closeTimer.current = setTimeout(hide, CLOSE_AFTER_SAVE_MS);
  }, [carry, hide]);

  /**
   * A spoken moment on a Return's line.
   *
   * `capture_voice` takes a recording, not a parent, so a continuation cannot be captured
   * as one. The link is made afterwards with `link_continuation` — the same command the
   * Record uses for "yes, that continues yesterday's" — rather than inventing a second way
   * for one moment to follow another.
   */
  const linkIfContinuing = useCallback(async (id: string) => {
    const value = returnedRef.current;
    if (!continuingRef.current || !value) return;
    await api.linkContinuation(id, value.fragment.id);
    await api.recordReturnOutcome(value.id, "continued");
    setReturned(null);
    setContinuing(false);
  }, []);

  const voiceOptions = useMemo(
    () => ({ origin: "menubar", releaseKey: "Alt", onFragment: linkIfContinuing }),
    [linkIfContinuing],
  );
  const voice = useVoice(afterCapture, voiceOptions);

  // ---- the window ------------------------------------------------------------------------

  useEffect(() => {
    // The panel is a second webview and reads none of the record's state, so the appearance
    // has to be applied here too — otherwise the menu bar opens a dark panel over a light
    // window, or the other way round.
    applyAppearance(readAppearance(), readLiftContrast());
    void applyStoredUiZoom().then((z) => {
      zoom.current = z;
    });
    document.documentElement.classList.add("tray-capture-page");
    // Deliberately not `readReturn()`: this webview mounts when the app launches, and
    // `select_return` surfaces as well as reads. A Return is something you arrive at, so it
    // is asked for when the panel opens and never merely because Chinotto is running.
    carry();
  }, [carry]);

  useEffect(() => {
    const win = getCurrentWindow();

    // Focus loss closes the panel, but only once the spurious blur has passed and the
    // window really is unfocused. Regaining focus puts the caret back in the field, so a
    // second open is typed into exactly like the first.
    const unlisten = win.onFocusChanged(({ payload: focused }) => {
      if (hideTimer.current) clearTimeout(hideTimer.current);
      if (focused) {
        fieldRef.current?.focus();
        return;
      }
      hideTimer.current = setTimeout(() => {
        void win.isFocused().then((still) => {
          if (!still) hide();
        });
      }, BLUR_HIDE_MS);
    });

    // The webview is never torn down between one open and the next, so "opening" has to be
    // told rather than inferred from mounting.
    const opened = listen("chinotto-tray-opened", () => {
      setFailure(null);
      setNote(null);
      readReturn();
      fieldRef.current?.focus();
    });

    // ⌥space anywhere on the mac, routed here rather than to the window while the panel is
    // the thing in front of you. Same rule as the panel's own ⌥: an empty field only.
    const held = listen("chinotto-voice-hold-start", () => {
      if (!fieldRef.current?.value) voice.start();
    });
    const released = listen("chinotto-voice-hold-stop", () => voice.stop());

    const t = setTimeout(() => fieldRef.current?.focus(), 30);
    return () => {
      clearTimeout(t);
      if (hideTimer.current) clearTimeout(hideTimer.current);
      if (closeTimer.current) clearTimeout(closeTimer.current);
      void unlisten.then((f) => f());
      void opened.then((f) => f());
      void held.then((f) => f());
      void released.then((f) => f());
    };
    // The two verbs, not the whole hook: `seconds` ticks while recording, and re-registering
    // the listeners on every tick both costs a round trip and can drop a release.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [hide, readReturn, voice.start, voice.stop]);

  /**
   * A recording holds the panel open.
   *
   * The blur debounce exists to dismiss a panel nobody is using; a panel that is listening
   * is being used, and macOS can take focus away for a moment while the microphone starts.
   */
  useEffect(() => {
    if (voice.recording && hideTimer.current) clearTimeout(hideTimer.current);
  }, [voice.recording]);

  /**
   * `esc` while speaking, and the caret afterwards.
   *
   * The waveform replaces the field rather than sitting beside it, so for as long as a
   * recording is running there is no focused element to take the key — it goes to the
   * window instead. When the recording ends the field comes back, and it comes back with
   * the caret in it, so a hold can be followed straight by typing.
   */
  useEffect(() => {
    if (!voice.recording) {
      fieldRef.current?.focus();
      return;
    }
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      voice.drop();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [voice.recording, voice.drop]);

  // The field grows with the text, as it does at the edge.
  useEffect(() => {
    const el = fieldRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, FIELD_MAX_HEIGHT)}px`;
  }, [text]);

  /**
   * The window is a frame around the panel, and the panel is not a fixed size — a second
   * line of words, a Return above the caret, the waveform, the interface at 85%. Measuring
   * what was drawn rather than predicting it is what keeps a two-line capture from being
   * cut off at the window's edge, and the panel under the glyph rather than left of it.
   */
  useEffect(() => {
    const panel = panelRef.current;
    if (!panel) return;
    const root = panel.parentElement;
    if (!root) return;
    const fit = () => {
      const box = panel.getBoundingClientRect();
      if (!box.height) return;
      // The root's padding is the shadow's room; it is drawn, so it is measured too.
      const style = getComputedStyle(root);
      const x = parseFloat(style.paddingLeft) + parseFloat(style.paddingRight);
      const y = parseFloat(style.paddingTop) + parseFloat(style.paddingBottom);
      // A window that would not resize costs a clipped line and nothing else — the words
      // are in the field either way — and there is nothing the person could do about it,
      // so it is not carried into the panel.
      void api
        .fitCapturePopover((box.width + x) * zoom.current, (box.height + y) * zoom.current)
        .catch(() => {});
    };
    const observer = new ResizeObserver(fit);
    observer.observe(panel);
    // The panel is measured again once the family has arrived: Archivo settles the hint
    // row's line box, and a window sized before that is a window sized to a fallback.
    void document.fonts?.ready.then(fit).catch(() => {});
    return () => observer.disconnect();
  }, []);

  // ---- leaving ---------------------------------------------------------------------------

  const leave = useCallback(async () => {
    const body = text.trim();
    if (!body) return;
    const value = returnedRef.current;
    const onLine = continuingRef.current && value ? value : null;
    // Cleared first, always. The panel must never look like it is asking permission.
    setText("");
    setFailure(null);
    try {
      const fragment = onLine
        ? await api.continueFragment(onLine.fragment.id, body, "typed", "menubar")
        : await api.captureFragment(body, "typed", "menubar");
      if (onLine) {
        await api.recordReturnOutcome(onLine.id, "continued").catch(() => {});
        setReturned(null);
        setContinuing(false);
      }
      void emit("chinotto-tray-entry-saved", { id: fragment.id });
      carry();
      setNote("left.");
      closeTimer.current = setTimeout(hide, CLOSE_AFTER_SAVE_MS);
    } catch (e) {
      // The one case where the panel stays: the words are still here, and saying so is the
      // only honest thing to do with them.
      setText(body);
      setFailure(shortReason(e));
      setNote(null);
    }
  }, [text, hide, carry]);

  const empty = text.length === 0;

  return (
    <div
      className="tray-capture-root"
      onMouseDown={(e) => {
        // Clicking the backdrop closes; clicking the panel does not.
        if (e.target === e.currentTarget) hide();
      }}
    >
      <div className="tray-capture-panel" ref={panelRef}>
        {returned && !voice.recording ? (
          <TrayReturn
            value={returned}
            continuing={continuing}
            onContinue={() => {
              setContinuing(true);
              fieldRef.current?.focus();
            }}
            onPutBack={() => {
              const value = returnedRef.current;
              if (value) void api.recordReturnOutcome(value.id, "let_go").catch(() => {});
              setReturned(null);
              setContinuing(false);
              carry();
            }}
          />
        ) : null}

        {voice.recording ? (
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: "16px",
              minHeight: "var(--tray-row-min)",
            }}
          >
            <SpeakingBars width="2px" height="var(--tray-caret-height)" />
            <span style={metaStyle("var(--size-meta-sm)")}>
              holding ⌥ · {durationLabel(voice.seconds)}
            </span>
          </div>
        ) : (
          <div
            style={{
              display: "flex",
              alignItems: "flex-start",
              gap: "var(--tray-row-gap)",
              minHeight: "var(--tray-row-min)",
              paddingTop: returned ? "12px" : undefined,
            }}
          >
            {/*
              The drawn bar is the caret's resting form, exactly as at the edge. The panel
              opens focused, so the field's own caret would stand beside it and two carets
              would be one too many: while the field is empty the system caret is held back
              and this bar is the caret. The first character swaps them over.
            */}
            {empty ? (
              <span
                aria-hidden="true"
                style={{
                  display: "inline-block",
                  width: "var(--caret-width)",
                  height: "var(--tray-caret-height)",
                  background: "var(--ink)",
                  marginTop: "2px",
                  flex: "none",
                }}
              />
            ) : null}
            <textarea
              ref={fieldRef}
              rows={1}
              value={text}
              onChange={(e) => setText(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Escape") {
                  e.preventDefault();
                  // One step at a time: a Return you armed goes back to waiting before the
                  // panel closes, so `esc` never silently answers it.
                  if (continuingRef.current) {
                    setContinuing(false);
                    return;
                  }
                  hide();
                  return;
                }
                // Hold ⌥ on an empty field to speak. `repeat` guards the key's own
                // auto-repeat, which would otherwise start a recording many times a second.
                if (e.key === "Alt" && empty && !e.repeat) {
                  e.preventDefault();
                  voice.start();
                  return;
                }
                if (e.key === "Enter" && !e.shiftKey) {
                  e.preventDefault();
                  void leave();
                }
              }}
              aria-label={continuing ? "Continue this" : "Leave a fragment"}
              placeholder={continuing ? "continue it" : "anything"}
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
                fontSize: "var(--tray-field)",
                lineHeight: 1.25,
                letterSpacing: "-0.012em",
                caretColor: empty ? "transparent" : "var(--ink)",
                padding: 0,
                overflowY: "auto",
                maxHeight: `${FIELD_MAX_HEIGHT}px`,
              }}
            />
          </div>
        )}

        <div
          style={{
            marginTop: "var(--tray-hint-top)",
            display: "flex",
            gap: "var(--tray-hint-gap)",
            ...metaStyle("var(--size-meta-sm)"),
            color: "var(--faint)",
          }}
        >
          {voice.recording ? (
            <>
              <span>release to leave it</span>
              <span style={{ marginLeft: "auto" }}>
                <Verb quiet onClick={voice.drop}>
                  esc drop it
                </Verb>
              </span>
            </>
          ) : (
            <>
              <span>{continuing ? "⏎ continue it" : "⏎ leave it"}</span>
              <span>{continuing ? "esc put it back" : "esc close"}</span>
              <span
                style={{
                  marginLeft: "auto",
                  color: failure ? "var(--ink-far)" : "var(--meta)",
                }}
              >
                {failure ?? note ?? (empty ? "hold ⌥ to speak" : "")}
              </span>
            </>
          )}
        </div>

        {voice.notice ? (
          <div
            style={{
              marginTop: "8px",
              fontSize: "var(--size-meta-sm)",
              lineHeight: 1.4,
              color: "var(--ink-far)",
              fontVariationSettings: "'wdth' 92",
            }}
          >
            {micSentence(voice.notice)}
          </div>
        ) : null}
      </div>
    </div>
  );
}

/**
 * The Return, at the panel's scale.
 *
 * It sits above the caret and never instead of it: the field stays where it was and keeps
 * the focus, because a Return is something the Record noticed rather than something it is
 * asking you to deal with before you may write. The rail, the three lines and the reasoning
 * are the Record's own — `becauseSentence` is imported rather than rewritten, so a Return
 * cannot read one way in the window and another way here.
 */
function TrayReturn({
  value,
  continuing,
  onContinue,
  onPutBack,
}: {
  value: api.ReturnValue;
  continuing: boolean;
  onContinue: () => void;
  onPutBack: () => void;
}) {
  const now = useNow();
  const from = new Date(value.fragment.capturedAt);
  const because = becauseSentence(value, now);
  const phraseOld = phraseOn(value.evidence, false);

  return (
    <div
      style={{
        position: "relative",
        paddingLeft: "16px",
        display: "flex",
        flexDirection: "column",
        gap: "5px",
        paddingBottom: "16px",
        marginBottom: "2px",
        borderBottom: "1px solid var(--rule-dim)",
        animation: "chinotto-rise var(--rise) var(--ease)",
      }}
    >
      <span
        aria-hidden="true"
        style={{
          position: "absolute",
          left: 0,
          top: "2px",
          bottom: "18px",
          width: "2px",
          background: "var(--rule-present)",
        }}
      />
      <span style={metaStyle("var(--size-meta-sm)")}>
        back from {fullDateLabel(from)} · {agoLabel(from, now)}
      </span>
      <span
        style={{
          fontSize: "17px",
          lineHeight: 1.25,
          letterSpacing: "-0.01em",
          color: "var(--ink)",
          textWrap: "pretty",
          overflowWrap: "anywhere",
        }}
      >
        <Marked text={value.fragment.body} mark={phraseOld} />
      </span>
      {because ? (
        <span
          style={{
            fontSize: "var(--size-meta)",
            lineHeight: 1.35,
            color: "var(--meta)",
            fontVariationSettings: "'wdth' 92",
            textWrap: "pretty",
          }}
        >
          {"when" in because ? (
            <>
              because {because.when} you wrote “
              <Marked text={because.quote ?? ""} mark={because.mark} />”
            </>
          ) : "opened" in because ? (
            <>because {because.opened}</>
          ) : (
            <>
              because {because.added} “<Marked text={because.quote} mark={because.mark} />”
            </>
          )}
        </span>
      ) : null}
      {continuing ? null : (
        <span style={{ marginTop: "7px", display: "flex", gap: "var(--tray-hint-gap)" }}>
          <Verb onClick={onContinue}>continue</Verb>
          <Verb quiet onClick={onPutBack}>
            put it back
          </Verb>
        </span>
      )}
    </div>
  );
}

/** What the microphone said, in the product's voice rather than the system's. */
function micSentence(notice: "ask" | "denied" | "failed"): string {
  if (notice === "denied") {
    return "the mac is not letting chinotto hear — system settings › privacy › microphone";
  }
  if (notice === "ask") return "the mac is asking about the microphone";
  return "that did not record";
}

function shortReason(e: unknown): string {
  const msg = e instanceof Error ? e.message : String(e);
  if (/not allowed on window|not allowed on webview|invoke/i.test(msg)) {
    return "this build cannot save from the menu bar yet";
  }
  return msg.length > 80 ? "it could not be saved — it is still here" : msg;
}
