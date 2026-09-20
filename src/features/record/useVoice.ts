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

export function useVoice(onCaptured: () => void) {
  const [recording, setRecording] = useState(false);
  const [seconds, setSeconds] = useState(0);
  const [notice, setNotice] = useState<VoiceState["notice"]>(null);
  const startedAt = useRef(0);
  const inflight = useRef(false);
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
        if (capture.durationMs < DROP_UNDER_MS) return;

        // The recording becomes a fragment first. If anything below fails, the audio is
        // already in the Record and can be transcribed or corrected later.
        const fragment = await api.captureVoice(capture.audioPath, capture.durationMs);
        try {
          await api.recordTranscript(
            fragment.id,
            capture.transcript,
            capture.transcript ? null : "nothing was transcribed",
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
  }, [onCaptured, clearNoticeLater]);

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

  const dismissNotice = useCallback(() => setNotice(null), []);

  // Memoised so `start` and `stop` keep their identity across the timer's ticks. The window
  // registers the ⌥space listeners against them, and a new pair every 100ms is a new pair of
  // native listeners every 100ms.
  return useMemo(
    () => ({ recording, seconds, notice, start, stop, dismissNotice }),
    [recording, seconds, notice, start, stop, dismissNotice],
  );
}
