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
//!
//! **Recognition happens on this Mac or not at all**, in every build. `SFSpeechRecognizer`
//! sends audio to Apple's servers unless a request both *requires* on-device recognition and
//! is made on a recogniser that *supports* it — Apple ignores the requirement where support
//! is missing. So every recognition task here is created by `on_device_task`, which refuses
//! unless both are true, and a request only exists once a local recogniser does: without
//! one no buffer is handed to Speech and the recording simply has no words yet. The direct
//! build used to fall back to the server; it no longer can.
//! `src/lib/voiceOnDeviceOnly.test.ts` reads this file to keep it that way.

#![cfg(target_os = "macos")]

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, Bool};
use objc2::AnyThread;
use objc2_avf_audio::{
    AVAudioApplication, AVAudioApplicationRecordPermission, AVAudioEngine, AVAudioFile,
    AVAudioFrameCount, AVAudioPCMBuffer, AVAudioTime,
};
use objc2_foundation::NSError;
use objc2_foundation::NSURL;
use objc2_speech::{
    SFSpeechAudioBufferRecognitionRequest, SFSpeechRecognitionRequest, SFSpeechRecognitionResult,
    SFSpeechRecognitionTask, SFSpeechRecognizer, SFSpeechRecognizerAuthorizationStatus,
    SFSpeechURLRecognitionRequest,
};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub use crate::transcript_retry::Reading;

const AUTHORIZED_STATUS: isize = 3;
const NOT_DETERMINED_STATUS: isize = 0;

/// True while a capture is running on the speech thread. Read-backs of retained recordings
/// stand aside for it.
static CAPTURING: AtomicBool = AtomicBool::new(false);

pub fn is_capturing() -> bool {
    CAPTURING.load(Ordering::SeqCst)
}

/// Set when the hold is released. `max_ms` is a ceiling, not the length of the recording.
pub static STOP_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Ends the current recording, if there is one. Idempotent.
pub fn request_stop() {
    STOP_REQUESTED.store(true, Ordering::SeqCst);
}

/// Clears any leftover release, at the moment a capture is asked for.
///
/// This has to happen here — before the command reaches the speech thread — and not inside
/// the capture. Starting a capture is slow the first time: the recogniser is created, the
/// audio engine starts, and the mac puts up its microphone prompt. That is precisely when a
/// person lets go of the key, and clearing the flag after all that threw their release away
/// and recorded until the two-minute ceiling instead. A release is never early; it is only
/// ever ahead of the machine.
pub fn arm_stop() {
    STOP_REQUESTED.store(false, Ordering::SeqCst);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionState {
    Granted,
    Denied,
    Undetermined,
    /// `AVAudioApplication` arrived in macOS 14. Older supported Macs keep the previous
    /// engine-triggered prompt rather than crashing while looking up a class they lack.
    Legacy,
}

pub fn microphone_permission_state() -> PermissionState {
    if AnyClass::get(c"AVAudioApplication").is_none() {
        return PermissionState::Legacy;
    }
    let app = unsafe { AVAudioApplication::sharedInstance() };
    match unsafe { app.recordPermission() } {
        AVAudioApplicationRecordPermission::Granted => PermissionState::Granted,
        AVAudioApplicationRecordPermission::Denied => PermissionState::Denied,
        AVAudioApplicationRecordPermission::Undetermined => PermissionState::Undetermined,
        _ => PermissionState::Denied,
    }
}

/// Present the microphone prompt. Must be called from the main thread; the answer is sent
/// back rather than waited for there, because the callback itself also needs the main run
/// loop to remain alive.
pub fn request_microphone_permission(answer: mpsc::SyncSender<bool>) {
    let handler = RcBlock::new(move |granted: Bool| {
        let _ = answer.try_send(granted.as_bool());
    });
    unsafe { AVAudioApplication::requestRecordPermissionWithCompletionHandler(&handler) };
}

pub fn speech_permission_state() -> PermissionState {
    match unsafe { SFSpeechRecognizer::authorizationStatus() }.0 {
        AUTHORIZED_STATUS => PermissionState::Granted,
        NOT_DETERMINED_STATUS => PermissionState::Undetermined,
        _ => PermissionState::Denied,
    }
}

/// Present the separate Speech Recognition prompt without starting a recording.
pub fn request_speech_permission(answer: mpsc::SyncSender<bool>) {
    let handler = RcBlock::new(move |status: SFSpeechRecognizerAuthorizationStatus| {
        let _ = answer.try_send(status.0 == AUTHORIZED_STATUS);
    });
    unsafe { SFSpeechRecognizer::requestAuthorization(&handler) };
}

/// What recognition did for one recording, or could do for one file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recognition {
    /// Recognised on this Mac.
    OnDevice,
    /// No on-device recogniser for this language, or none available right now. No task was made.
    Unavailable,
    /// Speech recognition is not authorised (or not yet decided). No task was made.
    Denied,
    /// Recognised on this Mac until the recogniser failed. The audio is unaffected.
    Failed,
}

impl Recognition {
    pub fn as_str(self) -> &'static str {
        match self {
            Recognition::OnDevice => "on_device",
            Recognition::Unavailable => "unavailable",
            Recognition::Denied => "denied",
            Recognition::Failed => "failed",
        }
    }
}

/// **The only way a recognition task is created in this module.**
///
/// `None` — and so no task, and no request anything is appended to — unless the recogniser
/// supports on-device recognition and the request requires it. Setting the requirement is
/// not enough on its own: Apple ignores it on a recogniser without support.
fn on_device_task(
    recognizer: &SFSpeechRecognizer,
    request: &SFSpeechRecognitionRequest,
    handler: &block2::DynBlock<dyn Fn(*mut SFSpeechRecognitionResult, *mut NSError)>,
) -> Option<Retained<SFSpeechRecognitionTask>> {
    if !unsafe { recognizer.supportsOnDeviceRecognition() } {
        return None;
    }
    unsafe { request.setRequiresOnDeviceRecognition(true) };
    if !unsafe { request.requiresOnDeviceRecognition() } {
        return None;
    }
    Some(unsafe { recognizer.recognitionTaskWithRequest_resultHandler(request, handler) })
}

/// Words from every recognition task that belongs to one uninterrupted recording.
///
/// Apple ends or resets a recognition task at phrase boundaries. A pause is therefore a
/// boundary in the derived transcript, not the end of the person's recording. Keeping the
/// committed prefix apart from the current task's partial prevents a fresh task from
/// replacing everything that was said before the pause.
#[derive(Default, Debug)]
struct TranscriptAccumulator {
    committed: String,
    current: String,
}

impl TranscriptAccumulator {
    fn observe(&mut self, text: &str) {
        let text = text.trim();
        // An empty terminal callback is common after useful partials. It is not a
        // correction and must not erase the last useful wording of this segment.
        if !text.is_empty() {
            self.current.clear();
            self.current.push_str(text);
        }
    }

    fn commit_segment(&mut self) {
        let segment = self.current.trim();
        if segment.is_empty() {
            return;
        }
        if !self.committed.is_empty() {
            self.committed.push(' ');
        }
        self.committed.push_str(segment);
        self.current.clear();
    }

    fn composed(&self) -> Option<String> {
        let committed = self.committed.trim();
        let current = self.current.trim();
        match (committed.is_empty(), current.is_empty()) {
            (true, true) => None,
            (false, true) => Some(committed.to_string()),
            (true, false) => Some(current.to_string()),
            (false, false) => Some(format!("{committed} {current}")),
        }
    }
}

#[derive(Debug)]
enum SegmentEvent {
    Words {
        generation: u64,
        text: String,
        final_result: bool,
    },
    Failed {
        generation: u64,
    },
}

#[derive(Default, Debug)]
struct PauseDetector {
    heard_audio: bool,
    silent_frames: u64,
    close_segment: bool,
}

/// RMS of the first input channel when AVAudioEngine gives us its normal float format.
/// Other formats still record and still use Speech's own phrase-final callbacks; they just
/// do not get the proactive pause boundary.
unsafe fn pcm_rms(buffer: &AVAudioPCMBuffer) -> Option<f32> {
    let channels = unsafe { buffer.floatChannelData() };
    if channels.is_null() {
        return None;
    }
    let frames = buffer.frameLength() as usize;
    if frames == 0 {
        return Some(0.0);
    }
    let stride = unsafe { buffer.stride() } as usize;
    let samples = unsafe { (*channels).as_ptr() };
    let mut squares = 0.0f32;
    for i in 0..frames {
        let sample = unsafe { *samples.add(i * stride) };
        squares += sample * sample;
    }
    Some((squares / frames as f32).sqrt())
}

/// Whether this Mac could read speech back as words locally right now. Asks nothing.
///
/// `available` · `unavailable` · `denied` · `not_determined`.
pub fn local_recognition_status() -> &'static str {
    let status = unsafe { SFSpeechRecognizer::authorizationStatus() };
    if status.0 == NOT_DETERMINED_STATUS {
        return "not_determined";
    }
    if status.0 != AUTHORIZED_STATUS {
        return "denied";
    }
    let Some(recognizer) = (unsafe { SFSpeechRecognizer::init(SFSpeechRecognizer::alloc()) })
    else {
        return "unavailable";
    };
    if unsafe { recognizer.supportsOnDeviceRecognition() && recognizer.isAvailable() } {
        "available"
    } else {
        "unavailable"
    }
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
        } else if current_status.0 == NOT_DETERMINED_STATUS {
            // The asking itself is done on the main thread, at the moment the capture was
            // asked for (see `lib.rs`). Nothing is waited for here: an answer that has not
            // arrived yet is not a failure of the recording.
            eprintln!(
                "[Speech] {} ms - warm-up: asked for authorisation; this recording keeps its audio",
                t0.elapsed().as_millis()
            );
            return Err(
                "the mac is being asked whether chinotto may read recordings back as words"
                    .to_string(),
            );
        } else {
            // Already denied (2) or restricted (1)
            current_status.0
        };

        if status != AUTHORIZED_STATUS {
            return Err(
                "the mac is not allowing chinotto to read recordings back as words \
                 · system settings › privacy & security › speech recognition"
                    .to_string(),
            );
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
            return Err(UNAVAILABLE_ON_DEVICE.to_string());
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
        let mut transcript_failure: Option<String> = None;
        let mut recognition = Recognition::OnDevice;
        let recognizer = match self.ensure_recognizer() {
            Ok(r) => Some(r),
            Err(e) => {
                eprintln!("[Speech] recogniser unavailable, recording anyway: {e}");
                // Authorisation is the other reason there is no recogniser; everything else
                // is this Mac not having one to give.
                recognition = if e.contains("not allowing") || e.contains("being asked") {
                    Recognition::Denied
                } else {
                    Recognition::Unavailable
                };
                // Kept, and handed back. "not transcribed" is true but says nothing about
                // what to do; the reason is the only part that is actionable.
                transcript_failure = Some(e);
                None
            }
        };

        // On-device or nothing, in every build. A Mac that cannot recognise this language
        // locally keeps the recording intact and gives it no words; nothing is sent to
        // Apple's speech service instead.
        let recognizer = recognizer.filter(|r| {
            let local = unsafe { r.supportsOnDeviceRecognition() };
            if !local {
                recognition = Recognition::Unavailable;
                transcript_failure = Some(UNAVAILABLE_ON_DEVICE.to_string());
            }
            local
        });

        let engine = unsafe { AVAudioEngine::new() };
        let input_node = unsafe { engine.inputNode() };
        let format = unsafe { input_node.outputFormatForBus(0) };

        // The file, opened before the tap is installed so no buffer can arrive with nowhere
        // to go. Written in the input's own format: this is the source, and resampling it
        // on the way in would mean the thing we kept is already a derivation.
        if let Some(parent) = audio_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let url = NSURL::fileURLWithPath(&objc2_foundation::NSString::from_str(
            &audio_path.to_string_lossy(),
        ));
        let settings = unsafe { format.settings() };
        let audio_file = unsafe {
            AVAudioFile::initForWriting_settings_error(AVAudioFile::alloc(), &url, &settings)
        }
        .map_err(|e| format!("could not open the recording for writing: {e:?}"))?;
        let frames_written = Arc::new(Mutex::new(0u64));
        let sample_rate = unsafe { format.sampleRate() };

        // The request the tap feeds. Filled in only once `on_device_task` has accepted it,
        // so without a local recogniser the tap writes the file and hands Speech nothing.
        let accepted: Arc<Mutex<Option<Retained<SFSpeechAudioBufferRecognitionRequest>>>> =
            Arc::new(Mutex::new(None));
        let request_for_tap = accepted.clone();
        let file_for_tap = audio_file.clone();
        let frames_for_tap = frames_written.clone();
        let pause_detector = Arc::new(Mutex::new(PauseDetector::default()));
        let pause_for_tap = pause_detector.clone();
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
                    if let Ok(guard) = request_for_tap.lock() {
                        if let Some(request) = guard.as_ref() {
                            request.appendAudioPCMBuffer(buf);
                        }
                    }

                    // A real pause closes only the recogniser's current phrase. The audio
                    // file and microphone remain continuous. This is the same separation
                    // used by the mobile recorder: the material does not inherit Speech's
                    // phrase-sized lifecycle.
                    if let Some(rms) = pcm_rms(buf) {
                        if let Ok(mut pause) = pause_for_tap.lock() {
                            const SPEECH_RMS: f32 = 0.022;
                            const SILENCE_RMS: f32 = 0.012;
                            const PHRASE_SILENCE_SECONDS: f64 = 1.6;
                            if rms > SPEECH_RMS {
                                pause.heard_audio = true;
                                pause.silent_frames = 0;
                            } else if pause.heard_audio && rms < SILENCE_RMS {
                                pause.silent_frames += buf.frameLength() as u64;
                                if sample_rate > 0.0
                                    && pause.silent_frames as f64 / sample_rate
                                        >= PHRASE_SILENCE_SECONDS
                                {
                                    pause.close_segment = true;
                                    pause.heard_audio = false;
                                    pause.silent_frames = 0;
                                }
                            } else {
                                pause.silent_frames = 0;
                            }
                        }
                    }
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

        let (segment_tx, segment_rx) = mpsc::channel::<SegmentEvent>();
        let transcript = Arc::new(Mutex::new(TranscriptAccumulator::default()));
        let first_partial_sent = Arc::new(Mutex::new(false));
        let start = Arc::new(Mutex::new(t0));
        let event_tx_shared = event_tx.map(|tx| Arc::new(Mutex::new(Some(tx))));
        let event_tx_for_sends = event_tx_shared.as_ref().map(Arc::clone);

        // Every task gets a generation. Cancelling the phrase before a pause often emits a
        // late terminal callback; generations keep that old callback from touching the
        // fresh phrase that now owns the microphone buffers.
        let generation = Arc::new(AtomicU64::new(0));
        let mut recognition_restarts = 0u16;
        const MAX_RECOGNITION_RESTARTS: u16 = 180;

        let make_segment = |recognizer: &SFSpeechRecognizer| {
            let this_generation = generation.fetch_add(1, Ordering::SeqCst) + 1;
            let tx = segment_tx.clone();
            let first_partial = first_partial_sent.clone();
            let started = start.clone();
            let result_block: RcBlock<
                dyn Fn(*mut SFSpeechRecognitionResult, *mut NSError) + 'static,
            > = RcBlock::new(
                move |result: *mut SFSpeechRecognitionResult, error: *mut NSError| {
                    if !error.is_null() {
                        eprintln!("[Speech] recognition segment ended with an error");
                        let _ = tx.send(SegmentEvent::Failed {
                            generation: this_generation,
                        });
                        return;
                    }
                    if result.is_null() {
                        return;
                    }
                    let result = unsafe { &*result };
                    let final_result = unsafe { result.isFinal() };
                    let text = unsafe { result.bestTranscription().formattedString() }.to_string();
                    let ms = started.lock().map(|t| t.elapsed().as_millis()).unwrap_or(0);
                    if final_result {
                        eprintln!("[Speech] {ms} ms - phrase final: {text}");
                    } else {
                        if let Ok(mut sent) = first_partial.lock() {
                            if !*sent {
                                *sent = true;
                                eprintln!("[Speech] {ms} ms - first partial: {text}");
                            }
                        }
                        eprintln!("[Speech] {ms} ms - phrase partial: {text}");
                    }
                    let _ = tx.send(SegmentEvent::Words {
                        generation: this_generation,
                        text,
                        final_result,
                    });
                },
            );
            let request = unsafe {
                SFSpeechAudioBufferRecognitionRequest::init(
                    SFSpeechAudioBufferRecognitionRequest::alloc(),
                )
            };
            unsafe { request.setShouldReportPartialResults(true) };
            let task = on_device_task(recognizer, &request, &result_block)?;
            Some((this_generation, request, task, result_block))
        };

        // No recogniser means no transcript, and that is a complete outcome rather than a
        // failure: the recording is still made, and words can be added to it later.
        let mut segment = recognizer.as_ref().and_then(|r| make_segment(r));
        if let Some((_, request, _, _)) = segment.as_ref() {
            eprintln!("[Speech] on-device recognition started");
            if let Ok(mut slot) = accepted.lock() {
                *slot = Some(request.clone());
            }
        }
        if recognizer.is_some() && segment.is_none() {
            recognition = Recognition::Unavailable;
            transcript_failure = Some(UNAVAILABLE_ON_DEVICE.to_string());
        }

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
        // ceiling, so a key that never reports its release cannot record forever. The flag
        // was armed by `arm_stop()` before this thread was even asked — see there for why
        // it must not be cleared here.
        let deadline = Instant::now() + Duration::from_millis(max_ms);
        while Instant::now() < deadline {
            if STOP_REQUESTED.swap(false, Ordering::SeqCst) {
                break;
            }

            let current_generation = segment.as_ref().map(|s| s.0);
            let mut restart_segment = false;
            while let Ok(event) = segment_rx.try_recv() {
                match event {
                    SegmentEvent::Words {
                        generation: event_generation,
                        text,
                        final_result,
                    } if Some(event_generation) == current_generation => {
                        if let Ok(mut words) = transcript.lock() {
                            words.observe(&text);
                            if final_result {
                                words.commit_segment();
                            }
                        }
                        restart_segment |= final_result;
                    }
                    SegmentEvent::Failed {
                        generation: event_generation,
                    } if Some(event_generation) == current_generation => {
                        // Apple uses an error as a phrase boundary on some locale/OS pairs.
                        // Preserve the partial and give the next phrase a fresh local task.
                        if let Ok(mut words) = transcript.lock() {
                            words.commit_segment();
                        }
                        restart_segment = true;
                    }
                    _ => {
                        // A cancelled task's late callback belongs to the preceding phrase.
                    }
                }
            }

            if let Ok(mut pause) = pause_detector.lock() {
                if pause.close_segment {
                    pause.close_segment = false;
                    if let Ok(mut words) = transcript.lock() {
                        words.commit_segment();
                    }
                    restart_segment = true;
                }
            }

            if restart_segment && segment.is_some() {
                recognition_restarts += 1;
                if let Some((_, old_request, old_task, old_handler)) = segment.take() {
                    if let Ok(mut slot) = accepted.lock() {
                        *slot = None;
                    }
                    unsafe {
                        old_request.endAudio();
                        old_task.cancel();
                    }
                    drop(old_handler);
                }

                segment = if recognition_restarts <= MAX_RECOGNITION_RESTARTS {
                    recognizer.as_ref().and_then(|r| make_segment(r))
                } else {
                    None
                };
                if let Some((_, request, _, _)) = segment.as_ref() {
                    if let Ok(mut slot) = accepted.lock() {
                        *slot = Some(request.clone());
                    }
                    if let Ok(mut pause) = pause_detector.lock() {
                        *pause = PauseDetector::default();
                    }
                    eprintln!(
                        "[Speech] {} ms - recognition continued after phrase {}",
                        t0.elapsed().as_millis(),
                        recognition_restarts
                    );
                } else {
                    recognition = Recognition::Failed;
                    transcript_failure.get_or_insert_with(|| {
                        "the local recogniser stopped before the recording did".to_string()
                    });
                    eprintln!("[Speech] recognition stopped; audio capture continues");
                }
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
        // Stop feeding Speech before ending its request. Keeping the tap alive while waiting
        // for a final result appended buffers after `endAudio`, which made the terminal
        // transcript timing-dependent.
        unsafe {
            engine.stop();
            input_node.removeTapOnBus(BUS);
        }
        if let Ok(mut slot) = accepted.lock() {
            *slot = None;
        }
        if let Some((_, request, _, _)) = segment.as_ref() {
            unsafe { request.endAudio() };
        }

        eprintln!(
            "[Speech] {} ms - waiting for final result",
            t0.elapsed().as_millis()
        );
        // Drain the last phrase. It is not restarted now: the recording itself has ended.
        if let Some((current_generation, _, _, _)) = segment.as_ref() {
            let final_deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < final_deadline {
                match segment_rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(SegmentEvent::Words {
                        generation: event_generation,
                        text,
                        final_result,
                    }) if event_generation == *current_generation => {
                        if let Ok(mut words) = transcript.lock() {
                            words.observe(&text);
                            if final_result {
                                words.commit_segment();
                            }
                        }
                        if final_result {
                            break;
                        }
                    }
                    Ok(SegmentEvent::Failed {
                        generation: event_generation,
                    }) if event_generation == *current_generation => {
                        if let Ok(mut words) = transcript.lock() {
                            words.commit_segment();
                        }
                        break;
                    }
                    Ok(_) => {}
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        }
        if let Some((_, _, task, handler)) = segment.take() {
            unsafe { task.cancel() };
            drop(handler);
        }
        drop(_tap_block_guard);

        // Close the file before anything reads it, so the header is complete on disk.
        drop(audio_file);

        let frames = frames_written.lock().map(|n| *n).unwrap_or(0);
        let duration_ms = if sample_rate > 0.0 {
            ((frames as f64 / sample_rate) * 1000.0).round() as u64
        } else {
            0
        };

        // From here the audio exists on disk whatever the recogniser did. Keep every
        // committed phrase plus the best partial from the last phrase.
        let transcript = transcript.lock().ok().and_then(|words| words.composed());
        if transcript.is_some() {
            if let Some(ref et) = event_tx_shared {
                if let Ok(guard) = et.lock() {
                    if let Some(ref tx) = *guard {
                        let _ = tx.send("voice_captured");
                    }
                }
            }
            // A recovered phrase-boundary error is not a failure of the final transcript.
            transcript_failure = None;
            recognition = Recognition::OnDevice;
        }
        if transcript.is_none() && transcript_failure.is_none() {
            transcript_failure = Some("nothing was heard in this recording".to_string());
        }

        Ok(VoiceCapture {
            audio_path: audio_path.to_path_buf(),
            duration_ms,
            transcript,
            transcript_failure,
            recognition,
        })
    }
}

/// Reads a retained recording back as words — on this Mac only, and without asking for
/// anything. Not on the speech thread: a capture must never wait behind a read-back, so the
/// caller checks `is_capturing` between recordings instead.
pub fn transcribe_file(audio_path: &std::path::Path) -> Reading {
    if !audio_path.is_file() {
        return Reading::Missing;
    }
    match local_recognition_status() {
        "available" => {}
        "unavailable" => return Reading::Unavailable,
        _ => return Reading::Denied,
    }
    let Some(recognizer) = (unsafe { SFSpeechRecognizer::init(SFSpeechRecognizer::alloc()) })
    else {
        return Reading::Unavailable;
    };

    let url = NSURL::fileURLWithPath(&objc2_foundation::NSString::from_str(
        &audio_path.to_string_lossy(),
    ));
    let request = unsafe {
        SFSpeechURLRecognitionRequest::initWithURL(SFSpeechURLRecognitionRequest::alloc(), &url)
    };
    unsafe { request.setShouldReportPartialResults(false) };

    let (tx, rx) = mpsc::sync_channel::<Reading>(1);
    let handler = RcBlock::new(
        move |result: *mut SFSpeechRecognitionResult, error: *mut NSError| {
            if !result.is_null() {
                let result = unsafe { &*result };
                if unsafe { result.isFinal() } {
                    let text = unsafe { result.bestTranscription().formattedString() }.to_string();
                    let text = text.trim().to_string();
                    let _ = tx.try_send(if text.is_empty() {
                        Reading::NoSpeech
                    } else {
                        Reading::Words(text)
                    });
                    return;
                }
            }
            if !error.is_null() {
                let error = unsafe { &*error };
                // 1110: no speech detected. An answer, not a failure.
                let no_speech =
                    error.domain().to_string() == "kAFAssistantErrorDomain" && error.code() == 1110;
                let _ = tx.try_send(if no_speech {
                    Reading::NoSpeech
                } else {
                    Reading::Failed
                });
            }
        },
    );

    let Some(task) = on_device_task(&recognizer, &request, &handler) else {
        return Reading::Unavailable;
    };
    // A recogniser that never answers must not hold the speech thread.
    let outcome = rx
        .recv_timeout(Duration::from_secs(120))
        .unwrap_or(Reading::Failed);
    unsafe { task.cancel() };
    drop(handler);
    outcome
}

/// Why a recording has no words when this Mac has no local recogniser for the language.
pub const UNAVAILABLE_ON_DEVICE: &str =
    "this mac can't turn speech into words on the device for this language · the recording is kept";

/// How long a recording on disk actually is.
///
/// Read from the file rather than guessed from its size: a `.caf` header can describe any
/// number of formats, and a recording's length is a fact about the material. `None` means
/// the file could not be opened at all, which is the one case where the app must not claim
/// a duration.
pub fn duration_ms_of(path: &std::path::Path) -> Option<u64> {
    let url = NSURL::fileURLWithPath(&objc2_foundation::NSString::from_str(
        &path.to_string_lossy(),
    ));
    let file = unsafe { AVAudioFile::initForReading_error(AVAudioFile::alloc(), &url) }.ok()?;
    let frames = unsafe { file.length() };
    let rate = unsafe { file.fileFormat().sampleRate() };
    if frames <= 0 || rate <= 0.0 {
        return None;
    }
    Some(((frames as f64 / rate) * 1000.0).round() as u64)
}

/// What one capture leaves behind: a recording, and possibly a reading of it.
#[derive(Clone, Debug)]
pub struct VoiceCapture {
    pub audio_path: std::path::PathBuf,
    pub duration_ms: u64,
    /// `None` means nothing was transcribed — not that the recording failed.
    pub transcript: Option<String>,
    /// Why there are no words, when that is known. Never a reason the *audio* failed.
    pub transcript_failure: Option<String>,
    /// Which path recognition took. There is no server path to report.
    pub recognition: Recognition,
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
        CAPTURING.store(true, Ordering::SeqCst);
        let result = manager.run_capture(max_ms, &audio_path, event_tx);
        CAPTURING.store(false, Ordering::SeqCst);
        let _ = result_tx.send(result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pause_keeps_the_first_phrase_when_the_next_task_starts_from_zero() {
        let mut words = TranscriptAccumulator::default();
        words.observe("primeira parte");
        words.commit_segment();

        // A fresh SFSpeechRecognitionTask knows only its own phrase.
        words.observe("segunda parte");
        assert_eq!(
            words.composed().as_deref(),
            Some("primeira parte segunda parte")
        );
    }

    #[test]
    fn empty_phrase_final_does_not_erase_its_last_useful_partial() {
        let mut words = TranscriptAccumulator::default();
        words.observe("uma ideia que leva tempo");
        words.observe("");
        words.commit_segment();

        assert_eq!(
            words.composed().as_deref(),
            Some("uma ideia que leva tempo")
        );
    }

    #[test]
    fn partial_corrections_replace_only_the_current_phrase() {
        let mut words = TranscriptAccumulator::default();
        words.observe("antes da pausa");
        words.commit_segment();
        words.observe("depois");
        words.observe("depois da pausa");

        assert_eq!(
            words.composed().as_deref(),
            Some("antes da pausa depois da pausa")
        );
    }

    /// A release belongs to the capture that was asked for before it.
    ///
    /// This ordering is the whole of the fix. `arm_stop` runs when the capture is *asked
    /// for*; everything after it is a release of that capture, including one that arrives
    /// while the recogniser is still being built or the mac is still asking about the
    /// microphone. Clearing the flag after all that — which is what the capture itself used
    /// to do — threw the release away and recorded to the two-minute ceiling instead.
    #[test]
    fn arming_clears_a_stale_release_and_keeps_every_later_one() {
        request_stop();
        arm_stop();
        assert!(
            !STOP_REQUESTED.load(Ordering::SeqCst),
            "a release from a previous capture must not end this one"
        );

        request_stop();
        assert!(
            STOP_REQUESTED.swap(false, Ordering::SeqCst),
            "a release asked for after arming must still be there when the loop looks"
        );
    }
}
