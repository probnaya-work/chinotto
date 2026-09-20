/**
 * Capture from the menu bar.
 *
 * `⌘⇧K` over whatever you are doing. The same 3px caret and the same field as the edge, at
 * a size that fits a panel rather than a window, and the same promise: the save is local
 * and instant, the field clears before anything else happens, and nothing about sync,
 * network or permission can be in the way.
 *
 * It writes into the Record — a fragment with `menubar` as its origin — rather than into
 * the legacy `entries` table it used to own. That is the difference between "quick capture
 * is a source here, and only here" being true and being a caption: the bridge still mirrors
 * the fragment out to mobile, but the Record is what it lands in.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { emit } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { captureFragment } from "@/lib/recordApi";
import { applyStoredUiZoom } from "@/lib/uiZoom";
import "../../design/tokens.css";

/**
 * After `show` / `set_focus`, macOS can deliver a spurious blur before focus settles.
 * Hiding on the first blur closes the panel the instant it opens.
 */
const BLUR_HIDE_MS = 280;

/** Long enough to read "left.", short enough that it never feels like waiting. */
const CLOSE_AFTER_SAVE_MS = 520;

export function TrayCapture() {
  const [text, setText] = useState("");
  const [note, setNote] = useState("lands in the record, dated now");
  const [failure, setFailure] = useState<string | null>(null);
  const fieldRef = useRef<HTMLTextAreaElement>(null);
  const hideTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const closeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const hide = useCallback(() => {
    void getCurrentWindow().hide();
  }, []);

  useEffect(() => {
    void applyStoredUiZoom();
    document.documentElement.classList.add("tray-capture-page");
    const win = getCurrentWindow();
    void win.setShadow(false).catch(() => {});

    // Focus loss closes the panel, but only once the spurious blur has passed and the
    // window really is unfocused.
    const unlisten = win.onFocusChanged(({ payload: focused }) => {
      if (hideTimer.current) clearTimeout(hideTimer.current);
      if (focused) return;
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
    el.style.height = `${el.scrollHeight}px`;
  }, [text]);

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

  return (
    <div
      className="tray-capture-root"
      onMouseDown={(e) => {
        // Clicking the backdrop closes; clicking the panel does not.
        if (e.target === e.currentTarget) hide();
      }}
    >
      <div className="tray-capture-panel">
        <div style={{ display: "flex", alignItems: "flex-start", gap: "14px" }}>
          {!text ? (
            <span
              aria-hidden="true"
              style={{
                display: "inline-block",
                width: "3px",
                height: "30px",
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
              fontSize: "24px",
              lineHeight: 1.25,
              letterSpacing: "-0.015em",
              caretColor: "var(--ink)",
              padding: 0,
              overflow: "hidden",
              maxHeight: "200px",
            }}
          />
        </div>

        <div
          style={{
            marginTop: "12px",
            display: "flex",
            gap: "22px",
            fontSize: "12px",
            color: "var(--faint-text)",
            fontVariationSettings: "'wdth' 90",
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
