/**
 * Capture from the menu bar.
 *
 * The identity file draws this surface directly: a 440pt panel under the glyph, one
 * hairline and a deep shadow, the edge's own caret and field at the panel's scale, and a
 * hint row of three. It is the app's edge moved to where the cursor already is — never a
 * list, never a search field, never a settings tree. Anything that wants more room opens
 * the window instead.
 *
 * The same promise as the edge: the save is local and instant, the field clears before
 * anything else happens, and nothing about sync, network or permission can be in the way.
 *
 * It writes into the Record — a fragment with `menubar` as its origin — rather than into
 * the legacy `entries` table it used to own. That is the difference between "quick capture
 * is a source here, and only here" being true and being a caption: the bridge still mirrors
 * the fragment out to mobile, but the Record is what it lands in.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { emit } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { captureFragment, fitCapturePopover } from "@/lib/recordApi";
import { metaStyle } from "../../design/tiers";
import { applyAppearance, readAppearance, readLiftContrast } from "@/lib/appearance";
import { applyStoredUiZoom } from "@/lib/uiZoom";
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
  const [note, setNote] = useState("lands in the record, dated now");
  const [failure, setFailure] = useState<string | null>(null);
  const fieldRef = useRef<HTMLTextAreaElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const hideTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const closeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  /**
   * The webview's own zoom. The panel measures itself in CSS pixels, which browser zoom
   * does not touch, so the window's logical size is that measurement times this.
   */
  const zoom = useRef(1);

  const hide = useCallback(() => {
    void getCurrentWindow().hide();
  }, []);

  useEffect(() => {
    // The panel is a second webview and reads none of the record's state, so the appearance
    // has to be applied here too — otherwise the menu bar opens a dark panel over a light
    // window, or the other way round.
    applyAppearance(readAppearance(), readLiftContrast());
    void applyStoredUiZoom().then((z) => {
      zoom.current = z;
    });
    document.documentElement.classList.add("tray-capture-page");
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

    const t = setTimeout(() => fieldRef.current?.focus(), 30);
    return () => {
      clearTimeout(t);
      if (hideTimer.current) clearTimeout(hideTimer.current);
      if (closeTimer.current) clearTimeout(closeTimer.current);
      void unlisten.then((f) => f());
    };
  }, [hide]);

  // The field grows with the text, as it does at the edge.
  useEffect(() => {
    const el = fieldRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, FIELD_MAX_HEIGHT)}px`;
  }, [text]);

  /**
   * The window is a frame around the panel, and the panel is not a fixed size — a second
   * line of words, a longer failure, the interface at 85%. Measuring what was drawn rather
   * than predicting it is what keeps a two-line capture from being cut off at the window's
   * edge, and the panel under the glyph rather than left of it.
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
      void fitCapturePopover((box.width + x) * zoom.current, (box.height + y) * zoom.current).catch(
        () => {},
      );
    };
    const observer = new ResizeObserver(fit);
    observer.observe(panel);
    // The panel is measured again once the family has arrived: Archivo settles the hint
    // row's line box, and a window sized before that is a window sized to a fallback.
    void document.fonts?.ready.then(fit).catch(() => {});
    return () => observer.disconnect();
  }, []);

  const leave = useCallback(async () => {
    const body = text.trim();
    if (!body) return;
    // Cleared first, always. The panel must never look like it is asking permission.
    setText("");
    setFailure(null);
    try {
      const fragment = await captureFragment(body, "typed", "menubar");
      setNote("left.");
      void emit("chinotto-tray-entry-saved", { id: fragment.id });
      closeTimer.current = setTimeout(hide, CLOSE_AFTER_SAVE_MS);
    } catch (e) {
      // The one case where the panel stays: the words are still here, and saying so is the
      // only honest thing to do with them.
      setText(body);
      setFailure(shortReason(e));
      setNote("");
    }
  }, [text, hide]);

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
        <div
          style={{
            display: "flex",
            alignItems: "flex-start",
            gap: "var(--tray-row-gap)",
            minHeight: "var(--tray-row-min)",
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
                hide();
                return;
              }
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                void leave();
              }
            }}
            aria-label="Leave a fragment"
            placeholder="anything"
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

        <div
          style={{
            marginTop: "var(--tray-hint-top)",
            display: "flex",
            gap: "var(--tray-hint-gap)",
            ...metaStyle("var(--size-meta-sm)"),
            color: "var(--faint)",
          }}
        >
          <span>⏎ leave it</span>
          <span>esc close</span>
          <span style={{ marginLeft: "auto", color: failure ? "var(--ink-far)" : "var(--meta)" }}>
            {failure ?? note}
          </span>
        </div>
      </div>
    </div>
  );
}

function shortReason(e: unknown): string {
  const msg = e instanceof Error ? e.message : String(e);
  if (/not allowed on window|not allowed on webview|invoke/i.test(msg)) {
    return "this build cannot save from the menu bar yet";
  }
  return msg.length > 80 ? "it could not be saved — it is still here" : msg;
}
