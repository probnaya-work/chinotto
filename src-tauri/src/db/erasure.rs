//! What is left of a voice moment once its removal can no longer be undone.
//!
//! ## Where "for good" is, on the Mac
//!
//! `remove_fragment` sets `removed_at`, deletes the legacy row and queues a tombstone, all at
//! once. What stands between that and permanence is only the quiet line's `bring back`,
//! offered for `UNDO_WINDOW_SECONDS` (8) and checked once a second — so gone within about
//! nine seconds, and gone at once if the app quits. Nothing else can restore a removed
//! fragment: `restore_fragment` has one caller, that button; remote tombstones never reach
//! the Record (`absorb_remote_deletes` has no caller); and the bridge never re-projects a
//! fragment it holds as removed.
//!
//! So a removal is permanent once `removed_at` is older than [`PERMANENT_AFTER_SECS`] — a
//! minute, well past anything the interface can still offer — and `restore_fragment` itself
//! now refuses past that point, so the rule is enforced here rather than merely observed.
//!
//! ## What goes, and what stays
//!
//! For a voice moment at that boundary: the recording, the machine transcript, the wording
//! it became and every earlier wording, the voice metadata, the search entry, the embedding,
//! and cached Traces and Returns that quote it. What stays is the `fragments` row — id,
//! `captured_at`, `capture_method`, `removed_at` — and the sync outbox and suppression rows
//! keyed on the id, which are what stop the moment coming back through sync.
//!
//! ## Order, because of `orphaned_recordings`
//!
//! A recording on disk that no `voice_captures` row claims is treated as lost and put back
//! in the Record at the next launch. So the file goes **first**, and the claim only after:
//! a file that cannot be deleted keeps its claim and is tried again, and a claim whose file
//! is already gone is simply erased. The reverse order would resurrect what was removed.
//!
//! `fragment_revisions` is otherwise append-only. This is its one deletion, and it is made
//! because the person asked for the moment to be gone.

use super::record::sync_fts;
use super::Db;
use rusqlite::OptionalExtension;

/// How long after a removal it becomes permanent. The interface offers eight seconds.
pub const PERMANENT_AFTER_SECS: i64 = 60;

/// The removal cutoff for `now`: anything removed at or before it is removed for good.
pub fn permanence_cutoff(now: chrono::DateTime<chrono::Utc>) -> String {
    (now - chrono::Duration::seconds(PERMANENT_AFTER_SECS)).to_rfc3339()
}

/// A removed-for-good voice moment whose content is still here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Erasable {
    pub fragment_id: String,
    /// Absolute, as `voice_captures` stores it. `None` when only a transcript row is left.
    pub audio_path: Option<String>,
}

impl Db {
    /// Voice moments removed at or before `cutoff` that still hold any voice content.
    pub fn removed_voice_to_erase(&self, cutoff: &str) -> Result<Vec<Erasable>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT f.id, v.audio_path FROM fragments f \
               LEFT JOIN voice_captures v ON v.fragment_id = f.id \
              WHERE f.removed_at IS NOT NULL \
                AND julianday(f.removed_at) <= julianday(?1) \
                AND (v.fragment_id IS NOT NULL \
                     OR EXISTS (SELECT 1 FROM voice_transcripts t WHERE t.fragment_id = f.id))",
        )?;
        let rows = stmt.query_map([cutoff], |r| {
            Ok(Erasable {
                fragment_id: r.get(0)?,
                audio_path: r.get(1)?,
            })
        })?;
        rows.collect()
    }

    /// Erases a removed voice moment's content, if it is removed for good by `cutoff`.
    ///
    /// The recording must already be gone from disk — see the module comment for why the
    /// file comes first. Idempotent: returns false when there was nothing to erase.
    pub fn erase_voice_content(&self, fragment_id: &str, cutoff: &str) -> Result<bool, rusqlite::Error> {
        let mut conn = self.0.lock().unwrap();
        let tx = conn.transaction()?;
        let eligible: Option<String> = tx
            .query_row(
                "SELECT f.id FROM fragments f \
                  WHERE f.id = ?1 AND f.removed_at IS NOT NULL \
                    AND julianday(f.removed_at) <= julianday(?2)",
                rusqlite::params![fragment_id, cutoff],
                |r| r.get(0),
            )
            .optional()?;
        if eligible.is_none() {
            return Ok(false);
        }
        // Only voice content is in scope, and a moment already erased has none left.
        let holds_voice: i64 = tx.query_row(
            "SELECT (SELECT COUNT(*) FROM voice_captures WHERE fragment_id = ?1) \
                  + (SELECT COUNT(*) FROM voice_transcripts WHERE fragment_id = ?1)",
            [fragment_id],
            |r| r.get(0),
        )?;
        if holds_voice == 0 {
            return Ok(false);
        }

        tx.execute("DELETE FROM fragment_revisions WHERE fragment_id = ?1", [fragment_id])?;
        // correction_count == COUNT(fragment_revisions) is an invariant.
        tx.execute(
            "UPDATE fragments SET body = '', correction_count = 0 WHERE id = ?1",
            [fragment_id],
        )?;
        tx.execute("DELETE FROM voice_transcripts WHERE fragment_id = ?1", [fragment_id])?;
        tx.execute("DELETE FROM voice_captures WHERE fragment_id = ?1", [fragment_id])?;
        tx.execute("DELETE FROM wording_conflicts WHERE fragment_id = ?1", [fragment_id])?;
        tx.execute("DELETE FROM fragment_embeddings WHERE fragment_id = ?1", [fragment_id])?;
        tx.execute(
            "DELETE FROM traces WHERE fragment_id = ?1 OR related_id = ?1",
            [fragment_id],
        )?;
        tx.execute(
            "DELETE FROM returns WHERE fragment_id = ?1 \
                OR id IN (SELECT return_id FROM return_evidence WHERE related_id = ?1)",
            [fragment_id],
        )?;
        // Removed material is already out of Find; this makes sure no row is left behind.
        sync_fts(&tx, fragment_id)?;
        tx.commit()?;
        Ok(true)
    }
}

/// Whether `path` is plainly one of this app's recordings: a `.caf` directly inside
/// `audio_dir`. A path comes from the database, and the database is not a reason to delete
/// something anywhere else.
pub fn is_retained_recording(path: &std::path::Path, audio_dir: &std::path::Path) -> bool {
    if path.extension().and_then(|e| e.to_str()) != Some("caf") {
        return false;
    }
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if name.starts_with('.') || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '_') {
        return false;
    }
    let (Ok(dir), Some(parent)) = (audio_dir.canonicalize(), path.parent()) else {
        return false;
    };
    match parent.canonicalize() {
        Ok(p) => p == dir,
        Err(_) => false,
    }
}

/// What one erasure pass did.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErasureReport {
    pub erased: usize,
    pub deleted_files: usize,
    /// Moments whose recording could not be deleted, or is not plainly ours. Kept, claim and
    /// all, and tried again next pass.
    pub kept: Vec<String>,
}

/// Erases every voice moment removed for good by `cutoff`: the file, then the content.
pub fn erase_removed_voice(db: &Db, audio_dir: &std::path::Path, cutoff: &str) -> ErasureReport {
    let mut report = ErasureReport::default();
    let Ok(due) = db.removed_voice_to_erase(cutoff) else {
        return report;
    };
    for item in due {
        if let Some(path) = item.audio_path.as_deref().map(std::path::Path::new) {
            if path.exists() {
                if !is_retained_recording(path, audio_dir) {
                    report.kept.push(item.fragment_id);
                    continue;
                }
                match std::fs::remove_file(path) {
                    Ok(()) => report.deleted_files += 1,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(_) => {
                        report.kept.push(item.fragment_id);
                        continue;
                    }
                }
            }
        }
        if let Ok(true) = db.erase_voice_content(&item.fragment_id, cutoff) {
            report.erased += 1;
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct Dir(PathBuf);
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn audio_dir() -> Dir {
        let d = std::env::temp_dir().join(format!("chinotto-erasure-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        Dir(d)
    }

    fn db() -> Db {
        Db::open(PathBuf::from(":memory:")).unwrap()
    }

    fn cutoff_after(secs: i64) -> String {
        permanence_cutoff(chrono::Utc::now() + chrono::Duration::seconds(secs))
    }

    /// A voice moment with words, a recording on disk, and a correction on top.
    fn voice_moment(db: &Db, dir: &std::path::Path) -> (String, PathBuf) {
        let path = dir.join(format!("{}.caf", uuid::Uuid::new_v4()));
        std::fs::write(&path, b"caff").unwrap();
        let f = db
            .capture_voice(&path.to_string_lossy(), 6_000, Some("desktop"), None)
            .unwrap();
        db.record_transcript(&f.id, Ok(("what was said".into(), "apple-on-device")))
            .unwrap();
        db.correct_fragment(&f.id, "what was meant").unwrap();
        (f.id, path)
    }

    fn count(db: &Db, sql: &str, id: &str) -> i64 {
        let conn = db.0.lock().unwrap();
        conn.query_row(sql, [id], |r| r.get(0)).unwrap()
    }

    #[test]
    fn nothing_is_erased_while_the_removal_can_still_be_brought_back() {
        let db = db();
        let dir = audio_dir();
        let (id, path) = voice_moment(&db, &dir.0);
        db.remove_fragment(&id).unwrap();

        let report = erase_removed_voice(&db, &dir.0, &permanence_cutoff(chrono::Utc::now()));
        assert_eq!(report, ErasureReport::default());
        assert!(path.exists());

        db.restore_fragment(&id).unwrap();
        let conn = db.0.lock().unwrap();
        let (body, removed): (String, Option<String>) = conn
            .query_row("SELECT body, removed_at FROM fragments WHERE id = ?1", [&id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(body, "what was meant");
        assert!(removed.is_none(), "bring back inside the window restores it");
    }

    #[test]
    fn a_removal_past_the_boundary_takes_the_recording_and_every_wording() {
        let db = db();
        let dir = audio_dir();
        let (id, path) = voice_moment(&db, &dir.0);
        db.remove_fragment(&id).unwrap();

        let report = erase_removed_voice(&db, &dir.0, &cutoff_after(PERMANENT_AFTER_SECS));
        assert_eq!(report.erased, 1);
        assert_eq!(report.deleted_files, 1);
        assert!(!path.exists());

        for table in ["voice_captures", "voice_transcripts", "fragment_revisions", "wording_conflicts"] {
            assert_eq!(
                count(&db, &format!("SELECT COUNT(*) FROM {table} WHERE fragment_id = ?1"), &id),
                0,
                "{table} still holds content"
            );
        }
        let conn = db.0.lock().unwrap();
        let row: (String, String, String, i64, Option<String>) = conn
            .query_row(
                "SELECT body, capture_method, captured_at, correction_count, removed_at FROM fragments WHERE id = ?1",
                [&id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap();
        assert_eq!(row.0, "", "no wording is left");
        assert_eq!(row.1, "voice");
        assert!(!row.2.is_empty(), "the moment keeps its date for sync");
        assert_eq!(row.3, 0);
        assert!(row.4.is_some(), "and stays removed");
        let tombstones: i64 = conn
            .query_row("SELECT COUNT(*) FROM sync_tombstone_outbox WHERE entry_id = ?1", [&id], |r| r.get(0))
            .unwrap();
        assert_eq!(tombstones, 1, "the tombstone is kept");
    }

    #[test]
    fn past_the_boundary_nothing_can_bring_it_back() {
        let db = db();
        let dir = audio_dir();
        let (id, _) = voice_moment(&db, &dir.0);
        db.remove_fragment(&id).unwrap();
        {
            let conn = db.0.lock().unwrap();
            conn.execute(
                "UPDATE fragments SET removed_at = ?1 WHERE id = ?2",
                rusqlite::params![(chrono::Utc::now() - chrono::Duration::seconds(PERMANENT_AFTER_SECS + 1)).to_rfc3339(), id],
            )
            .unwrap();
        }
        db.restore_fragment(&id).unwrap();
        assert_eq!(
            count(&db, "SELECT COUNT(*) FROM fragments WHERE id = ?1 AND removed_at IS NOT NULL", &id),
            1,
            "a restore past the boundary is refused"
        );
        assert_eq!(
            count(&db, "SELECT COUNT(*) FROM sync_tombstone_outbox WHERE entry_id = ?1", &id),
            1,
            "and the tombstone still goes"
        );
    }

    #[test]
    fn a_recording_already_gone_is_not_an_error_and_repeating_changes_nothing() {
        let db = db();
        let dir = audio_dir();
        let (id, path) = voice_moment(&db, &dir.0);
        std::fs::remove_file(&path).unwrap();
        db.remove_fragment(&id).unwrap();
        let cutoff = cutoff_after(PERMANENT_AFTER_SECS);

        let first = erase_removed_voice(&db, &dir.0, &cutoff);
        assert_eq!((first.erased, first.deleted_files), (1, 0));
        assert_eq!(erase_removed_voice(&db, &dir.0, &cutoff), ErasureReport::default());
        assert!(!db.erase_voice_content(&id, &cutoff).unwrap());
    }

    #[test]
    fn a_recording_that_is_not_plainly_ours_keeps_its_claim() {
        let db = db();
        let dir = audio_dir();
        let elsewhere = audio_dir();
        let path = elsewhere.0.join("x.caf");
        std::fs::write(&path, b"caff").unwrap();
        let f = db.capture_voice(&path.to_string_lossy(), 6_000, None, None).unwrap();
        db.remove_fragment(&f.id).unwrap();

        let report = erase_removed_voice(&db, &dir.0, &cutoff_after(PERMANENT_AFTER_SECS));
        assert_eq!(report.kept, vec![f.id.clone()]);
        assert!(path.exists());
        assert_eq!(
            count(&db, "SELECT COUNT(*) FROM voice_captures WHERE fragment_id = ?1", &f.id),
            1,
            "the claim stays, so the file is not treated as lost and adopted"
        );
    }

    #[test]
    fn typed_and_live_material_is_never_touched() {
        let db = db();
        let dir = audio_dir();
        let (live, live_path) = voice_moment(&db, &dir.0);
        let typed = db.capture_fragment("typed, not said", "typed", None).unwrap();
        db.remove_fragment(&typed.id).unwrap();

        let report = erase_removed_voice(&db, &dir.0, &cutoff_after(PERMANENT_AFTER_SECS));
        assert_eq!(report, ErasureReport::default());
        assert!(live_path.exists());
        assert_eq!(count(&db, "SELECT COUNT(*) FROM voice_captures WHERE fragment_id = ?1", &live), 1);
        let conn = db.0.lock().unwrap();
        let body: String = conn
            .query_row("SELECT body FROM fragments WHERE id = ?1", [&typed.id], |r| r.get(0))
            .unwrap();
        assert_eq!(body, "typed, not said");
    }

    #[test]
    fn only_a_caf_directly_in_the_audio_directory_is_ours() {
        let dir = audio_dir();
        let inside = dir.0.join("a.caf");
        std::fs::write(&inside, b"x").unwrap();
        assert!(is_retained_recording(&inside, &dir.0));
        assert!(!is_retained_recording(&dir.0.join("../a.caf"), &dir.0));
        assert!(!is_retained_recording(&dir.0.join("a.wav"), &dir.0));
        assert!(!is_retained_recording(&dir.0.join(".a.caf"), &dir.0));
        std::fs::create_dir_all(dir.0.join("sub")).unwrap();
        assert!(!is_retained_recording(&dir.0.join("sub/a.caf"), &dir.0));
    }
}
