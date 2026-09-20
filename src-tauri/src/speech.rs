//! Voice capture: the audio is the material, the transcript is derived from it.
//!
//! This used to be a transcript-only pipeline — the microphone fed the recogniser, the
//! recogniser produced a string, and the audio was discarded. That inverts the product's
//! model. A recording is something the person made; a transcript is a machine's reading of
//! it, which can be wrong, can fail entirely, and can be corrected later without the
//! recording changing at all. Throwing away the source to keep the derivation is the one
//! thing this module must not do.
//!
//! So the tap now feeds two things: the recogniser, and a file on disk. The file is written
//! as the buffers arrive, so an aborted recognition, a denied speech authorisation or a
//! crashed recogniser all still leave the audio behind. `run_capture` returns the path and
//! duration whether or not any words came back.

#![cfg(target_os = "macos")]

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::AnyThread;
use objc2_avf_audio::{
    AVAudioEngine, AVAudioFile, AVAudioFrameCount, AVAudioPCMBuffer, AVAudioTime,
};
use objc2_foundation::NSURL;
use objc2_foundation::NSError;
use objc2_speech::{
    SFSpeechAudioBufferRecognitionRequest, SFSpeechRecognitionResult, SFSpeechRecognizer,
};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const AUTHORIZED_STATUS: isize = 3;

/// Set when the hold is released. `max_ms` is a ceiling, not the length of the recording.
pub static STOP_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Ends the current recording, if there is one. Idempotent.
pub fn request_stop() {
    STOP_REQUESTED.store(true, Ordering::SeqCst);
}

/// Long-lived speech pipeline: authorization and recognizer created once.
pub struct SpeechManager {
    recognizer: Mutex<Option<Retained<SFSpeechRecognizer>>>,
}

impl SpeechManager {
    pub fn new() -> Self {
        Self {
            recognizer: Mutex::new(None),
        }
    }

    /// Ensure authorization and recognizer are ready. Call at app startup and/or before first capture.
    pub fn warm_up(&self) -> Result<(), String> {
        let guard = self.recognizer.lock().map_err(|e| e.to_string())?;
        if guard.is_some() {
            eprintln!("[Speech] Warm-up: already ready");
            return Ok(());
        }
        drop(guard);

        let t0 = Instant::now();

        // Check current authorization status first
        let current_status = unsafe { SFSpeechRecognizer::authorizationStatus() };
        eprintln!(
            "[Speech] {} ms - warm-up: current authorization status {}",
            t0.elapsed().as_millis(),
            current_status.0
        );

        let status: isize = if current_status.0 == AUTHORIZED_STATUS {
            eprintln!(
                "[Speech] {} ms - warm-up: already authorized",
                t0.elapsed().as_millis()
            );
            AUTHORIZED_STATUS
        } else if current_status.0 == 0 {
            // Status 0 = not determined, need to request authorization
            // However, requestAuthorization callback requires the main thread's run loop.
            // From a background thread, the callback never fires.
            // Instead of hanging, return an error with instructions.
            eprintln!(
                "[Speech] {} ms - warm-up: authorization not determined, needs user action",
                t0.elapsed().as_millis()
            );
            return Err("Speech recognition permission is required. \n\n\
                Please go to:\n\
                System Settings > Privacy & Security > Speech Recognition\n\n\
                Enable access for Chinotto, then try again."
                .to_string());
        } else {
            // Already denied (2) or restricted (1)
            current_status.0
        };

        if status != AUTHORIZED_STATUS {
            return Err(format!(
                "Speech recognition not authorized (status: {}).\n\n\
                Please go to:\n\
                System Settings > Privacy & Security > Speech Recognition\n\n\
                Enable access for Chinotto, then try again.",
                status
            ));
        }

        eprintln!(
            "[Speech] {} ms - warm-up: creating recognizer",
            t0.elapsed().as_millis()
        );
        let recognizer = unsafe {
            SFSpeechRecognizer::init(SFSpeechRecognizer::alloc())
                .ok_or("Could not create speech recognizer")?
        };
        let available = unsafe { recognizer.isAvailable() };
        eprintln!(
            "[Speech] {} ms - warm-up: recognizer available {}",
            t0.elapsed().as_millis(),
            available
        );
        if !available {
            return Err("Speech recognition is not available".to_string());
        }

        let mut guard = self.recognizer.lock().map_err(|e| e.to_string())?;
        *guard = Some(recognizer);
        eprintln!(
            "[Speech] {} ms - speech manager ready (warm-up done)",
            t0.elapsed().as_millis()
        );
        Ok(())
    }

    fn ensure_recognizer(&self) -> Result<Retained<SFSpeechRecognizer>, String> {
        let guard = self.recognizer.lock().map_err(|e| e.to_string())?;
        if let Some(r) = guard.as_ref() {
            return Ok(r.clone());
        }
        drop(guard);
        self.warm_up()?;
        let guard = self.recognizer.lock().map_err(|e| e.to_string())?;
        guard
            .as_ref()
            .cloned()
            .ok_or_else(|| "Recognizer missing after warm-up".to_string())
    }

    /// One capture: record up to `max_ms` to `audio_path`, and transcribe it if we can.
    ///
    /// The audio is written as it arrives, so it survives everything that can go wrong
    /// afterwards. The transcript is best-effort and may be `None`; the caller stores the
    /// recording either way.
    ///
    /// `event_tx`: "listening" | "processing" | "voice_captured" for the surface.
    pub fn run_capture(
        &self,
        max_ms: u64,
        audio_path: &std::path::Path,
        event_tx: Option<mpsc::SyncSender<&'static str>>,
    ) -> Result<VoiceCapture, String> {
        let t0 = Instant::now();
        eprintln!("[Speech] {} ms - capture started", t0.elapsed().as_millis());

        // Recognition is optional. If the Speech framework will not authorise — which is a
        // different permission from the microphone — the recording still happens and the
        // fragment is a voice fragment with no transcript yet.
        let recognizer = match self.ensure_recognizer() {
            Ok(r) => Some(r),
            Err(e) => {
                eprintln!("[Speech] recogniser unavailable, recording anyway: {e}");
                None
            }
        };

        let request = unsafe {
            SFSpeechAudioBufferRecognitionRequest::init(
                SFSpeechAudioBufferRecognitionRequest::alloc(),
            )
        };
        unsafe {
            request.setShouldReportPartialResults(true);
        }
        if let Some(ref r) = recognizer {
            if unsafe { r.supportsOnDeviceRecognition() } {
                unsafe { request.setRequiresOnDeviceRecognition(true) };
                eprintln!("[Speech] on-device recognition enabled");
            }
        }

        let engine = unsafe { AVAudioEngine::new() };
        let input_node = unsafe { engine.inputNode() };
        let format = unsafe { input_node.outputFormatForBus(0) };

        // The file, opened before the tap is installed so no buffer can arrive with nowhere
        // to go. Written in the input's own format: this is the source, and resampling it
        // on the way in would mean the thing we kept is already a derivation.
        if let Some(parent) = audio_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let url = unsafe { NSURL::fileURLWithPath(&objc2_foundation::NSString::from_str(
            &audio_path.to_string_lossy(),
        )) };
        let settings = unsafe { format.settings() };
        let audio_file = unsafe {
            AVAudioFile::initForWriting_settings_error(AVAudioFile::alloc(), &url, &settings)
        }
        .map_err(|e| format!("could not open the recording for writing: {e:?}"))?;
        let frames_written = Arc::new(Mutex::new(0u64));
        let sample_rate = unsafe { format.sampleRate() };

        let request_for_tap = request.clone();
        let file_for_tap = audio_file.clone();
        let frames_for_tap = frames_written.clone();
        let tap_block: RcBlock<dyn Fn(NonNull<AVAudioPCMBuffer>, NonNull<AVAudioTime>) + 'static> =
            RcBlock::new(
                move |buffer: NonNull<AVAudioPCMBuffer>, _when: NonNull<AVAudioTime>| unsafe {
                    let buf = buffer.as_ref();
                    // The recording first. If writing ever fails we keep going rather than
                    // tearing down the capture — a short recording beats none — but the
                    // failure is not swallowed silently either.
                    if let Err(e) = file_for_tap.writeFromBuffer_error(buf) {
                        eprintln!("[Speech] could not write audio buffer: {e:?}");
                    } else if let Ok(mut n) = frames_for_tap.lock() {
                        *n += buf.frameLength() as u64;
                    }
                    request_for_tap.appendAudioPCMBuffer(buf);
                },
            );

        const BUS: objc2_avf_audio::AVAudioNodeBus = 0;
        const BUFFER_SIZE: AVAudioFrameCount = 8192;
        let tap_block_ptr: *const _ = &*tap_block;
        unsafe {
            input_node.installTapOnBus_bufferSize_format_block(
                BUS,
                BUFFER_SIZE,
                Some(&format),
                tap_block_ptr as *mut _,
            );
        }
        let _tap_block_guard = tap_block;

        let (result_tx, result_rx) = mpsc::sync_channel::<Result<Option<String>, String>>(1);
        let last_transcript = Arc::new(Mutex::new(String::new()));
        let last_transcript_clone = last_transcript.clone();
        let first_partial_sent = Arc::new(Mutex::new(false));
        let start = Arc::new(Mutex::new(t0));
        let event_tx_shared = event_tx.map(|tx| Arc::new(Mutex::new(Some(tx))));
        let event_tx_for_sends = event_tx_shared.as_ref().map(Arc::clone);

        let result_block = RcBlock::new(
            move |result: *mut SFSpeechRecognitionResult, error: *mut NSError| {
                if !error.is_null() {
                    eprintln!("[Speech] Result handler got error");
                    let _ = result_tx.send(Ok(None));
                    return;
                }
                if result.is_null() {
                    return;
                }
                let result = unsafe { &*result };
                let is_final = unsafe { result.isFinal() };
                let transcription = unsafe { result.bestTranscription() };
                let s = unsafe { transcription.formattedString() };
                let text = s.to_string();

                let ms = start.lock().map(|t| t.elapsed().as_millis()).unwrap_or(0);
                if let Ok(mut last) = last_transcript_clone.lock() {
                    *last = text.clone();
                }

                if is_final {
                    eprintln!("[Speech] {} ms - final transcript received: {}", ms, text);
                    if let Some(ref et) = event_tx_shared {
                        if let Ok(guard) = et.lock() {
                            if let Some(ref tx) = *guard {
                                let _ = tx.send("voice_captured");
                            }
                        }
                    }
                    let _ = result_tx.send(Ok(Some(text)));
                } else {
                    if let Ok(mut sent) = first_partial_sent.lock() {
                        if !*sent {
                            *sent = true;
                            eprintln!(
                                "[Speech] {} ms - first partial transcript received: {}",
                                ms, text
                            );
                        }
                    }
                    eprintln!("[Speech] {} ms - partial: {}", ms, text);
                }
            },
        );

        // No recogniser means no transcript, and that is a complete outcome rather than a
        // failure: the recording is still made, and words can be added to it later.
        let task = recognizer.as_ref().map(|r| unsafe {
            r.recognitionTaskWithRequest_resultHandler(&request, &result_block)
        });

        let start_result = unsafe { engine.startAndReturnError() };
        if let Err(err) = start_result {
            unsafe {
                input_node.removeTapOnBus(BUS);
            }
            return Err(format!("Could not start audio engine: {}", err));
        }
        eprintln!(
            "[Speech] {} ms - audio engine started",
            t0.elapsed().as_millis()
        );
        if let Some(ref arc) = event_tx_for_sends {
            if let Ok(guard) = arc.lock() {
                if let Some(ref tx) = *guard {
                    let _ = tx.send("listening");
                }
            }
        }

        // Hold is the model, so the recording ends when the hold does. `max_ms` is only a
        // ceiling, so a key that never reports its release cannot record forever.
        STOP_REQUESTED.store(false, Ordering::SeqCst);
        let deadline = Instant::now() + Duration::from_millis(max_ms);
        while Instant::now() < deadline {
            if STOP_REQUESTED.swap(false, Ordering::SeqCst) {
                break;
            }
            std::thread::sleep(Duration::from_millis(16));
        }

        eprintln!("[Speech] {} ms - ending audio", t0.elapsed().as_millis());
        if let Some(ref arc) = event_tx_for_sends {
            if let Ok(guard) = arc.lock() {
                if let Some(ref tx) = *guard {
                    let _ = tx.send("processing");
                }
            }
        }
        unsafe {
            request.endAudio();
        }

        eprintln!(
            "[Speech] {} ms - waiting for final result",
            t0.elapsed().as_millis()
        );
        let outcome = result_rx.recv_timeout(Duration::from_millis(5000));

        unsafe {
            engine.stop();
            std::thread::sleep(Duration::from_millis(100));
            input_node.removeTapOnBus(BUS);
        }
        drop(task);
        drop(_tap_block_guard);

        // Close the file before anything reads it, so the header is complete on disk.
        drop(audio_file);

        let frames = frames_written.lock().map(|n| *n).unwrap_or(0);
        let duration_ms = if sample_rate > 0.0 {
            ((frames as f64 / sample_rate) * 1000.0).round() as u64
        } else {
            0
        };

        // From here the audio exists on disk whatever the recogniser did. A transcript that
        // failed, timed out or was never authorised produces `None`, never an error: there
        // is nothing to report a failure *about*, because the material is safe.
        let transcript = match outcome {
            Ok(Ok(t)) => t,
            Ok(Err(e)) => {
                eprintln!("[Speech] recognition failed, audio kept: {e}");
                None
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                eprintln!("[Speech] recognition timed out, using the best partial");
                last_transcript.lock().ok().and_then(|last| {
                    if last.is_empty() {
                        None
                    } else {
                        Some(last.clone())
                    }
                })
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => None,
        };

        Ok(VoiceCapture {
            audio_path: audio_path.to_path_buf(),
            duration_ms,
            transcript,
        })
    }
}

/// What one capture leaves behind: a recording, and possibly a reading of it.
#[derive(Clone, Debug)]
pub struct VoiceCapture {
    pub audio_path: std::path::PathBuf,
    pub duration_ms: u64,
    /// `None` means nothing was transcribed — not that the recording failed.
    pub transcript: Option<String>,
}

/// Command for the speech thread: (max_ms, where to write the audio, result, state events).
pub type SpeechCommand = (
    u64,
    std::path::PathBuf,
    mpsc::SyncSender<Result<VoiceCapture, String>>,
    Option<mpsc::SyncSender<&'static str>>,
);

/// Run the long-lived speech loop: warm up on first use, then handle capture commands.
pub fn run_speech_loop(rx: mpsc::Receiver<SpeechCommand>) {
    let manager = SpeechManager::new();
    eprintln!("[Speech] Speech loop started (warm-up deferred to first use)");
    while let Ok((max_ms, audio_path, result_tx, event_tx)) = rx.recv() {
        let result = manager.run_capture(max_ms, &audio_path, event_tx);
        let _ = result_tx.send(result);
    }
}
