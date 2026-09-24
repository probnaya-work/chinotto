/**
 * Hold to speak.
 *
 * The recording is the material. It is written to disk as it arrives and becomes a fragment
 * the moment it ends, whether or not any words came back — so a recogniser that is
 * unauthorised, fails, or simply hears nothing costs the transcript and never the audio.
 * The transcript is attached afterwards, as derived material, and can be corrected later
 * without the recording changing at all.
 *
 * Released under `DROP_UNDER_MS` and nothing is kept: that is a slip of the hand, not a
 * thought, and the product should not make somebody delete it.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import * as api from "../../lib/recordApi";

/** A press this short is a slip, not something said. */
export const DROP_UNDER_MS = 800;

export type MicState = "granted" | "ask" | "denied";

export interface VoiceState {
  /** Recording now. */
  recording: boolean;
  /** Seconds elapsed, for the meter at capture size. */
  seconds: number;
  /** A sentence the edge has to say, once. */
  notice: "ask" | "denied" | "failed" | null;
}

export interface VoiceOptions {
  /**
   * Where the recording was made. `menubar` for the panel under the glyph, so a fragment
   * spoken there is as honestly sourced as one typed there.
   */
  origin?: string;
  /**
   * The key whose release ends the hold. `space` at the edge, where the field is empty and
   * the space bar is free; `Alt` in the menu-bar panel, which the identity file draws as
   * `hold ⌥ to speak` and which needs no chord because the panel already has focus.
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
  const [seconds, setSeconds] = useState(0);
  const [notice, setNotice] = useState<VoiceState["notice"]>(null);
  const startedAt = useRef(0);
  const inflight = useRef(false);
  /**
   * This hold was a mistake and must not be kept.
   *
   * The same branch a release under `DROP_UNDER_MS` takes: the native capture is stopped
   * and the result never becomes a fragment. It is not a deletion and it does not touch the
   * product's stance that a kept recording is the material — nothing was kept. The file the
   * native side already wrote stays where it is, and `orphaned_recordings` is what finds it
   * again if it mattered after all.
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
  const start = useCallback(() => {
    if (inflight.current) return;
    inflight.current = true;
    dropped.current = false;
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
        if (dropped.current || capture.durationMs < DROP_UNDER_MS) return;

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
        setRecording(false);
        setSeconds(0);
        if (tick.current) clearInterval(tick.current);
      });
  }, [onCaptured, clearNoticeLater, origin, onFragment]);

  /**
   * Ends the recording.
   *
   * The native side stops on its own when the hold is released; this is the frontend's
   * half of the gesture, and it exists so `esc` can drop a recording as well.
   */
  const stop = useCallback(() => {
    if (!inflight.current) return;
    // The native side is holding the microphone open; this is what closes it. `max_ms` is
    // only a ceiling, so a release that never arrives cannot record forever.
    void api.stopVoiceCapture().catch(() => {});
  }, []);

  /**
   * The release, wherever it happens.
   *
   * Both gestures that start a recording are presses — `space` in the field, and the mouse
   * on `◌ hold space to speak` — and both used to listen for the release on the element
   * that took the press. Let the pointer slide off the words, or let focus go, and the
   * release lands somewhere else and is never seen: the recording then runs to its ceiling,
   * two minutes later, with no way to end it. A hold is held against the window.
   *
   * `blur` counts as a release for the same reason. A window that is no longer frontmost
   * will not be told when the key comes up, so continuing to record would mean recording
   * past the only moment we could have stopped.
   */
  const stopRef = useRef(stop);
  stopRef.current = stop;
  useEffect(() => {
    if (!recording) return;
    const onKeyUp = (e: KeyboardEvent) => {
      if (e.key === releaseKey) stopRef.current();
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
      if (e.key === releaseKey) e.preventDefault();
    };
    const release = () => stopRef.current();
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);
    window.addEventListener("mouseup", release);
    window.addEventListener("blur", release);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
      window.removeEventListener("mouseup", release);
      window.removeEventListener("blur", release);
    };
  }, [recording, releaseKey]);

  /** `esc` while speaking. Ends the hold, and keeps nothing. */
  const drop = useCallback(() => {
    if (!inflight.current) return;
    dropped.current = true;
    void api.stopVoiceCapture().catch(() => {});
  }, []);

  const dismissNotice = useCallback(() => setNotice(null), []);

  // Memoised so `start` and `stop` keep their identity across the timer's ticks. The window
  // registers the ⌥space listeners against them, and a new pair every 100ms is a new pair of
  // native listeners every 100ms.
  return useMemo(
    () => ({ recording, seconds, notice, start, stop, drop, dismissNotice }),
    [recording, seconds, notice, start, stop, drop, dismissNotice],
  );
}
