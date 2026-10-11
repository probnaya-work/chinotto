/**
 * Hold to speak.
 *
 * The recording is the material. It is written to disk as it arrives and becomes a fragment
 * the moment it ends, whether or not any words came back — so a recogniser that is
 * unauthorised, fails, or simply hears nothing costs the transcript and never the audio.
 * The transcript is attached afterwards, as derived material, and can be corrected later
 * without the recording changing at all.
 *
 * A short release locks the microphone hands-free; a longer hold is left when released.
 * Nothing is discarded implicitly. `esc` is the one reliable way to keep nothing.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import * as api from "../../lib/recordApi";

/** Orphaned files below this are incomplete starts, not recoverable recordings. */
export const DROP_UNDER_MS = 800;
/** A release before this turns a press into a hands-free recording. */
export const TAP_TO_LOCK_MS = 800;

export type MicState = "granted" | "ask" | "denied";

export interface VoiceState {
  /** Recording now. */
  recording: boolean;
  /** The press has been released, but the recording intentionally continues. */
  locked: boolean;
  /** Seconds elapsed, for the meter at capture size. */
  seconds: number;
  /** A sentence the edge has to say, once. */
  notice: "ask" | "ready" | "denied" | "failed" | null;
}

export interface VoiceOptions {
  /**
   * Where the recording was made. `menubar` for the panel under the glyph, so a fragment
   * spoken there is as honestly sourced as one typed there.
   */
  origin?: string;
  /**
   * The key whose release either locks a tap or finishes a hold. `space` at the edge, where
   * the field is empty and the space bar is free; `Alt` in the menu-bar panel, which needs
   * no chord because the panel already has focus.
   */
  releaseKey?: string;
  /**
   * Called with the fragment the recording became, before the transcript is attached.
   * The panel uses it to link a continuation: voice cannot be captured *as* a continuation
   * — `capture_voice` takes a recording, not a parent — so the link is made after, with
   * the same `link_continuation` the Record uses for "yes, that continues yesterday's".
   */
  onFragment?: (id: string) => Promise<void> | void;
}

/**
 * The recogniser a capture's transcript names, or null.
 *
 * Words are labelled on-device only when the Mac said recognition ran there. A capture that
 * the local recogniser listened to all the way through and heard nothing in is labelled too
 * — that is an answer, and it is not asked again. Anything else is left unlabelled, which is
 * what keeps it waiting for a local reading.
 */
export function transcriptModel(capture: api.VoiceCaptureResult): string | null {
  const local = capture.recognition === "on_device" || capture.recognition === "failed";
  if (capture.transcript) return local ? api.ON_DEVICE_MODEL : null;
  return capture.recognition === "on_device" ? api.ON_DEVICE_MODEL : null;
}

export function useVoice(onCaptured: () => void, options: VoiceOptions = {}) {
  const { origin = "desktop", releaseKey = " ", onFragment } = options;
  const [recording, setRecording] = useState(false);
  const [locked, setLocked] = useState(false);
  const [seconds, setSeconds] = useState(0);
  const [notice, setNotice] = useState<VoiceState["notice"]>(null);
  const startedAt = useRef(0);
  const inflight = useRef(false);
  const lockedRef = useRef(false);
  const stopRequested = useRef(false);
  const arming = useRef(false);
  const releasedWhileArming = useRef(false);
  const cancelledWhileArming = useRef(false);
  const [preparing, setPreparing] = useState(false);
  /**
   * This hold was a mistake and must not be kept.
   *
   * It is not a deletion and it does not touch the product's stance that a kept recording
   * is the material — the person explicitly said to keep nothing. The file the native side
   * already wrote stays where it is, and `orphaned_recordings` can diagnose an interrupted
   * capture without silently turning an accidental start into a thought.
   */
  const dropped = useRef(false);
  const tick = useRef<ReturnType<typeof setInterval> | null>(null);
  const noticeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const clearNoticeLater = useCallback((ms: number) => {
    if (noticeTimer.current) clearTimeout(noticeTimer.current);
    noticeTimer.current = setTimeout(() => setNotice(null), ms);
  }, []);

  useEffect(
    () => () => {
      if (tick.current) clearInterval(tick.current);
      if (noticeTimer.current) clearTimeout(noticeTimer.current);
    },
    [],
  );

  /**
   * Starts recording.
   *
   * The Rust side owns the whole capture: it returns when the release is seen, with the
   * path, the duration and whatever the recogniser managed. There is nothing to poll and
   * nothing that can leave a half-finished recording behind.
   */
  const beginCapture = useCallback((initiallyLocked = false) => {
    if (inflight.current) return;
    inflight.current = true;
    stopRequested.current = false;
    dropped.current = false;
    lockedRef.current = initiallyLocked;
    setLocked(initiallyLocked);
    startedAt.current = Date.now();
    setRecording(true);
    setSeconds(0);
    setNotice(null);
    tick.current = setInterval(
      () => setSeconds((Date.now() - startedAt.current) / 1000),
      100,
    );

    void api
      .recordVoice()
      .then(async (capture) => {
        if (dropped.current) {
          // `esc` is final. Renaming/removing on the native side also keeps this file out of
          // orphan recovery if deletion itself is interrupted.
          await api.discardVoiceRecording(capture.audioPath);
          return;
        }

        // The recording becomes a fragment first. If anything below fails, the audio is
        // already in the Record and can be transcribed or corrected later.
        const fragment = await api.captureVoice(capture.audioPath, capture.durationMs, origin);
        // Before the transcript, because the link is about the recording and must survive a
        // recogniser that fails.
        try {
          await onFragment?.(fragment.id);
        } catch {
          // A link that could not be made leaves an ordinary fragment in the Record, which
          // is the material intact and only the relation lost.
        }
        try {
          await api.recordTranscript(
            fragment.id,
            capture.transcript,
            // The mac's own reason, when it gave one. "not transcribed" is true and says
            // nothing; "the mac is not allowing chinotto to read recordings back as words"
            // is the difference between a dead end and a setting.
            capture.transcript
              ? null
              : (capture.transcriptFailure ?? "nothing was transcribed"),
            transcriptModel(capture),
          );
        } catch {
          // A transcript that could not be stored is a missing reading, not a missing
          // recording. The fragment stands.
        }
        onCaptured();
        // This Mac just recognised locally, so recordings still waiting for words need not
        // wait out a backoff from when it could not.
        if (capture.recognition === "on_device") {
          void api
            .retryTranscripts(true)
            .then((r) => {
              if (r.transcribed > 0) onCaptured();
            })
            .catch(() => {});
        }
      })
      .catch((e) => {
        const message = String(e);
        if (/denied|not authorized|not allowed/i.test(message)) {
          setNotice("denied");
          clearNoticeLater(7000);
        } else if (/authoriz|permission|not determined/i.test(message)) {
          setNotice("ask");
        } else {
          setNotice("failed");
          clearNoticeLater(7000);
        }
      })
      .finally(() => {
        inflight.current = false;
        stopRequested.current = false;
        lockedRef.current = false;
        setLocked(false);
        setRecording(false);
        setSeconds(0);
        if (tick.current) clearInterval(tick.current);
      });
  }, [onCaptured, clearNoticeLater, origin, onFragment]);

  /** Finish and keep exactly once, whether it came from release, click, key or blur. */
  const finish = useCallback(() => {
    if (arming.current) {
      releasedWhileArming.current = true;
      cancelledWhileArming.current = true;
      return;
    }
    if (!inflight.current || stopRequested.current) return;
    stopRequested.current = true;
    void api.stopVoiceCapture().catch(() => {
      stopRequested.current = false;
    });
  }, []);

  const start = useCallback(() => {
    if (inflight.current) {
      // A second press is the clearest possible end to a hands-free recording.
      if (lockedRef.current) finish();
      return;
    }
    if (arming.current) return;
    arming.current = true;
    releasedWhileArming.current = false;
    cancelledWhileArming.current = false;
    dropped.current = false;
    setPreparing(true);
    setNotice(null);

    void api
      .prepareVoiceAccess()
      .then((access) => {
        if (access.microphone === "denied") {
          setNotice("denied");
          clearNoticeLater(7000);
          return;
        }
        if (access.prompted) {
          // The permission sheet consumed this gesture and the intended audio happened
          // before macOS supplied buffers. Starting now would create the one-second junk
          // fragment seen on first use. Permission is ready; the next hold is the recording.
          setNotice("ready");
          clearNoticeLater(5000);
          return;
        }
        if (cancelledWhileArming.current) return;
        // A quick tap can end while the local permission check is still crossing the Tauri
        // bridge. With access already granted that is the lock gesture, not a gesture to
        // lose. A real system sheet reports `prompted` above and never enters this branch.
        const lockOnStart = releasedWhileArming.current;
        arming.current = false;
        setPreparing(false);
        beginCapture(lockOnStart);
      })
      .catch((e) => {
        const message = String(e);
        if (/denied|not authorized|not allowed/i.test(message)) {
          setNotice("denied");
          clearNoticeLater(7000);
        } else {
          setNotice("failed");
          clearNoticeLater(7000);
        }
      })
      .finally(() => {
        arming.current = false;
        setPreparing(false);
      });
  }, [beginCapture, clearNoticeLater, finish]);

  /**
   * Resolves a release: a tap becomes hands-free; a hold finishes and is kept.
   */
  const release = useCallback(() => {
    if (arming.current) {
      releasedWhileArming.current = true;
      return;
    }
    if (!inflight.current) return;
    if (lockedRef.current || stopRequested.current) return;
    if (Date.now() - startedAt.current < TAP_TO_LOCK_MS) {
      lockedRef.current = true;
      setLocked(true);
      return;
    }
    finish();
  }, [finish]);

  /**
   * The release, wherever it happens.
   *
   * Both gestures that start a recording are presses — `space` in the field, and the mouse
   * on `◌ hold space to speak` — and both used to listen for the release on the element
   * that took the press. Let the pointer slide off the words, or let focus go, and the
   * release lands somewhere else and is never seen: the recording then runs to its ceiling,
   * two minutes later, with no way to end it. A hold is held against the window.
   *
   * `blur` finishes outright rather than locking. A window that is no longer frontmost
   * will not be told when the key comes up, so continuing hands-free would mean recording
   * past the only moment we could have stopped.
   */
  const releaseRef = useRef(release);
  releaseRef.current = release;
  const finishRef = useRef(finish);
  finishRef.current = finish;
  useEffect(() => {
    if (!recording && !preparing) return;
    const onKeyUp = (e: KeyboardEvent) => {
      if (e.key === releaseKey) releaseRef.current();
    };
    /**
     * The held key is still the page's key.
     *
     * `space` is how a browser pages down, and the field that swallowed the first press is
     * gone the moment the recording starts — both surfaces replace it with the waveform
     * rather than hiding it behind one. So every auto-repeat of the hold lands on the
     * document instead, and the record pages itself to the far end underneath a recording
     * nobody has finished making yet. The hold is held against the window; so is its
     * default.
     */
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key !== releaseKey) return;
      e.preventDefault();
      if (lockedRef.current && !e.repeat) finishRef.current();
    };
    const releasePointer = () => releaseRef.current();
    const finishOnBlur = () => finishRef.current();
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);
    window.addEventListener("mouseup", releasePointer);
    window.addEventListener("blur", finishOnBlur);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
      window.removeEventListener("mouseup", releasePointer);
      window.removeEventListener("blur", finishOnBlur);
    };
  }, [recording, preparing, releaseKey]);

  /** `esc` while speaking. Ends the hold, and keeps nothing. */
  const drop = useCallback(() => {
    if (arming.current) {
      releasedWhileArming.current = true;
      cancelledWhileArming.current = true;
      dropped.current = true;
      return;
    }
    if (!inflight.current) return;
    dropped.current = true;
    finish();
  }, [finish]);

  const dismissNotice = useCallback(() => setNotice(null), []);

  // Memoised so the verbs keep their identity across the timer's ticks. The window
  // registers the ⌥space listeners against them, and a new pair every 100ms is a new pair of
  // native listeners every 100ms.
  return useMemo(
    () => ({ recording, locked, seconds, notice, start, stop: release, finish, drop, dismissNotice }),
    [recording, locked, seconds, notice, start, release, finish, drop, dismissNotice],
  );
}
