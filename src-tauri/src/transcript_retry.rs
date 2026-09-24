//! Reading retained recordings back as words, later, on this Mac only.
//!
//! A recording made while this Mac had no local recogniser for the language — or before
//! speech recognition was allowed — keeps its audio and has no words. Nothing is sent
//! anywhere to get them. When the Mac says it can recognise locally, each such recording is
//! read back once, from its own file, through the same on-device-only gate a live capture
//! uses (`speech::transcribe_file`).
//!
//!   * **Not blind.** A pass starts by asking whether local recognition is available, which
//!     asks nothing of the person. If it is not, nothing is attempted and the next check
//!     waits longer than the last one.
//!   * **Once per recording.** A recogniser that actually ran records `ON_DEVICE_MODEL` on
//!     the transcript, words or not, and such a transcript is never picked up again. A
//!     recording that could not be tried is left exactly as it was.
//!   * **One pass at a time**, and never in a capture's way: the pass stops at the next
//!     recording when a capture starts, and a capture never waits for a read-back.
//!   * **Nothing new is made**, and a result is only written if the moment still wants it
//!     (`Db::record_local_reading`).

use crate::db::material::RetryCandidate;
use crate::db::Db;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// A `pending` transcript older than this was orphaned by a quit between capture and settle.
pub const PENDING_STALE_SECS: i64 = 10 * 60;
/// First wait after a pass that found no local recognition, then doubling, capped.
pub const BACKOFF_MIN_MS: i64 = 5 * 60_000;
pub const BACKOFF_MAX_MS: i64 = 24 * 60 * 60_000;
/// How many recordings one pass reads. The rest wait for the next pass.
pub const BATCH: i64 = 20;

pub const NOTHING_HEARD: &str = "nothing was heard in this recording";
pub const RECOGNISER_FAILED: &str = "the on-device recogniser stopped";

/// What reading one recording back came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reading {
    Words(String),
    NoSpeech,
    Unavailable,
    Denied,
    Failed,
    Missing,
}

/// What one pass did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RetryPass {
    Skipped { why: String },
    Ran {
        transcribed: usize,
        heard_nothing: usize,
        still_waiting: usize,
        /// Fragments that now have words, for mirroring to sync.
        changed: Vec<String>,
    },
}

/// When the next pass may look.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Schedule {
    backoff_ms: i64,
    not_before_ms: i64,
}

impl Schedule {
    fn back_off(&mut self, now_ms: i64) {
        self.backoff_ms = if self.backoff_ms == 0 {
            BACKOFF_MIN_MS
        } else {
            (self.backoff_ms * 2).min(BACKOFF_MAX_MS)
        };
        self.not_before_ms = now_ms + self.backoff_ms;
    }

    fn settle(&mut self) {
        self.backoff_ms = 0;
        self.not_before_ms = 0;
    }
}

/// The platform, as a pass needs it.
pub trait Recogniser {
    /// `available` · `unavailable` · `denied` · `not_determined`. Never prompts.
    fn status(&self) -> &'static str;
    /// Local-only reading of one file. Never prompts.
    fn read(&self, audio_path: &std::path::Path) -> Reading;
    /// A capture is running; read-backs stand aside.
    fn capturing(&self) -> bool;
}

/// One pass over the recordings waiting for words.
pub fn run_pass(
    db: &Db,
    recogniser: &dyn Recogniser,
    schedule: &mut Schedule,
    now: chrono::DateTime<chrono::Utc>,
) -> RetryPass {
    let now_ms = now.timestamp_millis();
    if now_ms < schedule.not_before_ms {
        return RetryPass::Skipped { why: "backing-off".into() };
    }
    if recogniser.capturing() {
        return RetryPass::Skipped { why: "capturing".into() };
    }
    let status = recogniser.status();
    if status != "available" {
        schedule.back_off(now_ms);
        return RetryPass::Skipped { why: status.into() };
    }

    let pending_before = (now - chrono::Duration::seconds(PENDING_STALE_SECS)).to_rfc3339();
    let candidates: Vec<RetryCandidate> =
        db.voice_retry_candidates(&pending_before, BATCH).unwrap_or_default();

    let (mut transcribed, mut heard_nothing, mut still_waiting) = (0, 0, 0);
    let mut changed = Vec::new();
    for c in candidates {
        if recogniser.capturing() {
            still_waiting += 1;
            continue;
        }
        let path = std::path::Path::new(&c.audio_path);
        if !path.is_file() {
            let _ = db.mark_audio_missing(&c.fragment_id);
            continue;
        }
        let result = match recogniser.read(path) {
            Reading::Words(text) => Ok(text),
            Reading::NoSpeech => Err(NOTHING_HEARD.to_string()),
            Reading::Failed => Err(RECOGNISER_FAILED.to_string()),
            // Nothing was tried. The recording stays exactly as it was, and waits.
            Reading::Unavailable | Reading::Denied => {
                still_waiting += 1;
                continue;
            }
            Reading::Missing => continue,
        };
        let words = result.is_ok();
        if let Ok(true) = db.record_local_reading(&c, result) {
            if words {
                transcribed += 1;
                changed.push(c.fragment_id.clone());
            } else {
                heard_nothing += 1;
            }
        }
    }

    if still_waiting > 0 {
        schedule.back_off(now_ms);
    } else {
        schedule.settle();
    }
    RetryPass::Ran { transcribed, heard_nothing, still_waiting, changed }
}

static RUNNING: AtomicBool = AtomicBool::new(false);
static SCHEDULE: Mutex<Schedule> = Mutex::new(Schedule { backoff_ms: 0, not_before_ms: 0 });

/// Runs a pass unless one is already running. `fresh` forgets the backoff — for when a
/// capture has just recognised locally, so nothing waiting need wait out an old miss.
pub fn run_once(db: &Db, recogniser: &dyn Recogniser, fresh: bool) -> RetryPass {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return RetryPass::Skipped { why: "running".into() };
    }
    let pass = {
        let mut schedule = SCHEDULE.lock().unwrap_or_else(|e| e.into_inner());
        if fresh {
            schedule.settle();
        }
        run_pass(db, recogniser, &mut schedule, chrono::Utc::now())
    };
    RUNNING.store(false, Ordering::SeqCst);
    pass
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::material::ON_DEVICE_MODEL;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::path::PathBuf;

    struct Fake {
        status: RefCell<&'static str>,
        readings: RefCell<HashMap<String, Reading>>,
        read: RefCell<Vec<String>>,
        capturing: RefCell<bool>,
        on_read: RefCell<Option<Box<dyn Fn()>>>,
    }

    impl Fake {
        fn new() -> Self {
            Fake {
                status: RefCell::new("available"),
                readings: RefCell::new(HashMap::new()),
                read: RefCell::new(Vec::new()),
                capturing: RefCell::new(false),
                on_read: RefCell::new(None),
            }
        }
    }

    impl Recogniser for Fake {
        fn status(&self) -> &'static str {
            *self.status.borrow()
        }
        fn read(&self, path: &std::path::Path) -> Reading {
            let key = path.to_string_lossy().into_owned();
            self.read.borrow_mut().push(key.clone());
            if let Some(f) = self.on_read.borrow().as_ref() {
                f();
            }
            self.readings
                .borrow()
                .get(&key)
                .cloned()
                .unwrap_or_else(|| Reading::Words(format!("words of {key}")))
        }
        fn capturing(&self) -> bool {
            *self.capturing.borrow()
        }
    }

    struct Dir(PathBuf);
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn dir() -> Dir {
        let d = std::env::temp_dir().join(format!("chinotto-retry-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        Dir(d)
    }

    fn db() -> Db {
        Db::open(PathBuf::from(":memory:")).unwrap()
    }

    /// A voice moment with no words, as a capture without a local recogniser leaves it.
    fn waiting(db: &Db, dir: &std::path::Path, failure: &str) -> (String, String) {
        let path = dir.join(format!("{}.caf", uuid::Uuid::new_v4()));
        std::fs::write(&path, b"caff").unwrap();
        let path = path.to_string_lossy().into_owned();
        let f = db.capture_voice(&path, 5_000, Some("desktop"), None).unwrap();
        db.record_transcript(&f.id, Err(failure.into())).unwrap();
        (f.id, path)
    }

    fn transcript(db: &Db, id: &str) -> (String, Option<String>, Option<String>) {
        let v = db.voice_for(id).unwrap().unwrap();
        (v.transcript_state, db.transcript_model(id).unwrap(), v.machine_transcript)
    }

    fn now() -> chrono::DateTime<chrono::Utc> {
        chrono::Utc::now()
    }

    #[test]
    fn reads_back_a_recording_that_had_no_local_recogniser() {
        let (db, d, fake) = (db(), dir(), Fake::new());
        let (id, path) = waiting(&db, &d.0, "no on-device recogniser for this language");
        let mut s = Schedule::default();

        let pass = run_pass(&db, &fake, &mut s, now());

        assert!(matches!(pass, RetryPass::Ran { transcribed: 1, heard_nothing: 0, still_waiting: 0, .. }));
        assert_eq!(*fake.read.borrow(), vec![path.clone()]);
        let (state, model, words) = transcript(&db, &id);
        assert_eq!(state, "ok");
        assert_eq!(model.as_deref(), Some(ON_DEVICE_MODEL));
        assert_eq!(words, Some(format!("words of {path}")));
    }

    #[test]
    fn asks_first_and_touches_nothing_when_local_recognition_is_unavailable() {
        let (db, d, fake) = (db(), dir(), Fake::new());
        let (id, _) = waiting(&db, &d.0, "nothing was heard in this recording");
        *fake.status.borrow_mut() = "unavailable";
        let mut s = Schedule::default();

        assert_eq!(run_pass(&db, &fake, &mut s, now()), RetryPass::Skipped { why: "unavailable".into() });
        assert!(fake.read.borrow().is_empty());
        assert_eq!(transcript(&db, &id).1, None, "still waiting, untouched");

        *fake.status.borrow_mut() = "denied";
        let later = now() + chrono::Duration::milliseconds(BACKOFF_MAX_MS);
        assert_eq!(run_pass(&db, &fake, &mut s, later), RetryPass::Skipped { why: "denied".into() });
    }

    #[test]
    fn backs_off_doubling_up_to_a_day() {
        let (db, fake) = (db(), Fake::new());
        *fake.status.borrow_mut() = "unavailable";
        let mut s = Schedule::default();
        let t0 = now();

        run_pass(&db, &fake, &mut s, t0);
        let inside = t0 + chrono::Duration::milliseconds(BACKOFF_MIN_MS - 1);
        assert_eq!(run_pass(&db, &fake, &mut s, inside), RetryPass::Skipped { why: "backing-off".into() });
        let t1 = t0 + chrono::Duration::milliseconds(BACKOFF_MIN_MS);
        run_pass(&db, &fake, &mut s, t1);
        assert_eq!(s.backoff_ms, BACKOFF_MIN_MS * 2);

        let mut t = t1;
        for _ in 0..20 {
            t = t + chrono::Duration::milliseconds(BACKOFF_MAX_MS);
            run_pass(&db, &fake, &mut s, t);
        }
        assert_eq!(s.backoff_ms, BACKOFF_MAX_MS);
    }

    #[test]
    fn once_per_recording_whatever_the_reading_said() {
        let (db, d, fake) = (db(), dir(), Fake::new());
        let (quiet, quiet_path) = waiting(&db, &d.0, "x");
        let (broken, broken_path) = waiting(&db, &d.0, "x");
        fake.readings.borrow_mut().insert(quiet_path.clone(), Reading::NoSpeech);
        fake.readings.borrow_mut().insert(broken_path.clone(), Reading::Failed);
        let mut s = Schedule::default();

        let pass = run_pass(&db, &fake, &mut s, now());
        assert!(matches!(pass, RetryPass::Ran { transcribed: 0, heard_nothing: 2, .. }));
        run_pass(&db, &fake, &mut s, now());
        assert_eq!(fake.read.borrow().len(), 2, "neither is read twice");
        assert_eq!(transcript(&db, &quiet).1.as_deref(), Some(ON_DEVICE_MODEL));
        assert_eq!(transcript(&db, &broken).1.as_deref(), Some(ON_DEVICE_MODEL));
    }

    #[test]
    fn a_recording_it_could_not_try_keeps_waiting_and_backs_off() {
        let (db, d, fake) = (db(), dir(), Fake::new());
        let (id, path) = waiting(&db, &d.0, "x");
        fake.readings.borrow_mut().insert(path, Reading::Unavailable);
        let mut s = Schedule::default();

        let pass = run_pass(&db, &fake, &mut s, now());
        assert!(matches!(pass, RetryPass::Ran { still_waiting: 1, .. }));
        assert_eq!(transcript(&db, &id).1, None);
        assert_eq!(run_pass(&db, &fake, &mut s, now()), RetryPass::Skipped { why: "backing-off".into() });
    }

    #[test]
    fn stands_aside_for_a_capture() {
        let (db, d, fake) = (db(), dir(), Fake::new());
        waiting(&db, &d.0, "x");
        *fake.capturing.borrow_mut() = true;
        let mut s = Schedule::default();
        assert_eq!(run_pass(&db, &fake, &mut s, now()), RetryPass::Skipped { why: "capturing".into() });
        assert!(fake.read.borrow().is_empty());
    }

    #[test]
    fn a_moment_removed_while_its_file_was_read_does_not_get_the_words() {
        let db = std::rc::Rc::new(db());
        let (d, fake) = (dir(), Fake::new());
        let (id, _) = waiting(&db, &d.0, "x");
        let (db2, id2) = (db.clone(), id.clone());
        *fake.on_read.borrow_mut() = Some(Box::new(move || db2.remove_fragment(&id2).unwrap()));
        let mut s = Schedule::default();

        run_pass(&db, &fake, &mut s, now());
        let (state, model, words) = transcript(&db, &id);
        assert_eq!((state.as_str(), model, words), ("failed", None, None));
    }

    #[test]
    fn missing_audio_is_recorded_and_never_read() {
        let (db, d, fake) = (db(), dir(), Fake::new());
        let (id, path) = waiting(&db, &d.0, "x");
        std::fs::remove_file(&path).unwrap();
        run_pass(&db, &fake, &mut Schedule::default(), now());
        assert!(fake.read.borrow().is_empty());
        assert!(db.voice_for(&id).unwrap().unwrap().audio_missing);
    }

    #[test]
    fn a_fresh_pending_transcript_is_mid_capture_and_left_alone() {
        let (db, d, fake) = (db(), dir(), Fake::new());
        let path = d.0.join("p.caf");
        std::fs::write(&path, b"caff").unwrap();
        db.capture_voice(&path.to_string_lossy(), 5_000, None, None).unwrap();
        run_pass(&db, &fake, &mut Schedule::default(), now());
        assert!(fake.read.borrow().is_empty());
        let later = now() + chrono::Duration::seconds(PENDING_STALE_SECS + 1);
        run_pass(&db, &fake, &mut Schedule::default(), later);
        assert_eq!(fake.read.borrow().len(), 1);
    }

    #[test]
    fn transcripts_that_already_have_words_are_left_alone() {
        let (db, d, fake) = (db(), dir(), Fake::new());
        let path = d.0.join("w.caf");
        std::fs::write(&path, b"caff").unwrap();
        let f = db.capture_voice(&path.to_string_lossy(), 5_000, None, None).unwrap();
        db.record_transcript(&f.id, Ok(("already said".into(), "apple-speech"))).unwrap();
        run_pass(&db, &fake, &mut Schedule::default(), now());
        assert!(fake.read.borrow().is_empty());
    }
}
