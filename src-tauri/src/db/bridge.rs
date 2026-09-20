//! Temporary compatibility boundary between the Record and legacy `entries`.
//!
//! The Record is canonical on desktop. `entries` survives only because the shipped mobile
//! app and the Firestore sync protocol speak it, and neither can be changed from here. This
//! module is the whole of that compatibility surface, and it is meant to be deleted.
//!
//! ## The one rule
//!
//! The legacy model is `{ id, text, created_at }`. It cannot express a Line, a correction
//! history, retained audio or an encounter. So the bridge **degrades** — it sends the parts
//! that fit and leaves the rest behind — and never **fabricates**: it does not invent a
//! capture method it was not told, does not flatten two dated moments into one entry with
//! appended text, and does not synthesise revisions from a text change arriving over sync.
//!
//! ## Loops
//!
//! Both directions are keyed on the same id, which makes the id the idempotency token:
//!
//! * a fragment mirrored out keeps its id, so when that row comes back through
//!   `ingest_firestore_entries` (an `INSERT OR IGNORE`) it is skipped;
//! * an entry projected in keeps its id, so a second projection finds the fragment already
//!   there and does nothing.
//!
//! Neither direction can re-trigger the other. There is no sequence number, no clock
//! comparison and no conflict resolution, because the protocol has no incoming-edit path:
//! ingest only ever inserts rows that do not exist yet.

use super::Db;

impl Db {
    /// Mirrors one fragment into `entries` so the existing sync can carry it to mobile.
    ///
    /// Called after every write the Record makes. Idempotent: mirroring twice updates the
    /// same row rather than creating a second one.
    ///
    /// A fragment with an empty body is skipped — a voice capture exists before its
    /// transcript does, and `ingest_firestore_entries` rejects empty text anyway. It is
    /// mirrored later, when there is something to mirror.
    pub fn mirror_fragment_to_entry(&self, fragment_id: &str) -> Result<bool, rusqlite::Error> {
        let conn = self.0.lock().unwrap();

        let row: Option<(String, String, Option<String>)> = conn
            .query_row(
                "SELECT body, captured_at, removed_at FROM fragments WHERE id = ?1",
                [fragment_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .ok();

        let Some((body, captured_at, removed_at)) = row else {
            return Ok(false);
        };
        if removed_at.is_some() || body.trim().is_empty() {
            return Ok(false);
        }

        // Never resurrect something the user removed on another device.
        let suppressed: i64 = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM firestore_ingest_suppressed_ids WHERE id = ?1)",
            [fragment_id],
            |r| r.get(0),
        )?;
        if suppressed != 0 {
            return Ok(false);
        }

        // updated_at moves so the legacy row reflects the current wording after a
        // correction. edit_count is deliberately NOT touched: it is v1's own count of
        // destructive edits, and the Record's correction history lives in
        // fragment_revisions where it keeps the wording it replaced.
        conn.execute(
            "INSERT INTO entries (id, text, created_at, updated_at) VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(id) DO UPDATE SET text = excluded.text, updated_at = excluded.updated_at",
            rusqlite::params![
                fragment_id,
                body.trim(),
                captured_at,
                chrono::Utc::now().to_rfc3339()
            ],
        )?;
        Ok(true)
    }

    /// Projects legacy entries that have no fragment into the Record.
    ///
    /// Run after an ingest. Returns how many arrived.
    ///
    /// `capture_method` is `imported` and `capture_origin` is `mobile`: the sync payload is
    /// `{id, text, created_at}` and says nothing about how the thing was captured, so the
    /// Record records that it does not know rather than claiming `typed`.
    pub fn project_entries_into_record(&self) -> Result<usize, rusqlite::Error> {
        let pending: Vec<(String, String, String)> = {
            let conn = self.0.lock().unwrap();
            let mut stmt = conn.prepare(
                "SELECT e.id, e.text, e.created_at FROM entries e \
                 LEFT JOIN fragments f ON f.id = e.id \
                 WHERE f.id IS NULL AND TRIM(e.text) <> ''",
            )?;
            let rows: Vec<(String, String, String)> = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<Result<_, _>>()?;
            rows
        };
        if pending.is_empty() {
            return Ok(0);
        }

        let conn = self.0.lock().unwrap();
        let mut arrived = 0usize;
        for (id, text, created_at) in pending {
            let n = conn.execute(
                "INSERT OR IGNORE INTO fragments \
                   (id, body, captured_at, capture_method, capture_origin, legacy_entry_id) \
                 VALUES (?1, ?2, ?3, 'imported', 'mobile', ?1)",
                rusqlite::params![id, text, created_at],
            )?;
            if n > 0 {
                super::record::sync_fts(&conn, &id)?;
                arrived += 1;
            }
        }
        Ok(arrived)
    }

    /// Removes a fragment from the Record and from the legacy row sync reads.
    ///
    /// Soft on the Record's side — `removed_at` is set, the material stays — and hard on the
    /// legacy side, because that is the only thing the sync protocol can express. The
    /// suppression id stops the next pull from re-inserting it, and the tombstone tells
    /// other devices.
    pub fn remove_fragment(&self, fragment_id: &str) -> Result<(), rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "UPDATE fragments SET removed_at = ?1 WHERE id = ?2 AND removed_at IS NULL",
            rusqlite::params![now, fragment_id],
        )?;
        // Removed material stops answering Find but stays in the Record.
        super::record::sync_fts(&conn, fragment_id)?;

        conn.execute("DELETE FROM entries WHERE id = ?1", [fragment_id])?;
        conn.execute(
            "INSERT OR REPLACE INTO firestore_ingest_suppressed_ids (id, suppressed_at) VALUES (?1, ?2)",
            rusqlite::params![fragment_id, now],
        )?;
        conn.execute(
            "INSERT OR REPLACE INTO sync_tombstone_outbox (entry_id, enqueued_at) VALUES (?1, ?2)",
            rusqlite::params![fragment_id, now],
        )?;
        Ok(())
    }

    /// Puts back something removed by mistake, on this device.
    ///
    /// Clears the suppression and the pending tombstone so the restored fragment can mirror
    /// out again. If the tombstone has already been delivered this will not reach other
    /// devices — an honest limit of the legacy protocol, not something to paper over.
    pub fn restore_fragment(&self, fragment_id: &str) -> Result<(), rusqlite::Error> {
        {
            let conn = self.0.lock().unwrap();
            conn.execute(
                "UPDATE fragments SET removed_at = NULL WHERE id = ?1",
                [fragment_id],
            )?;
            super::record::sync_fts(&conn, fragment_id)?;
            conn.execute(
                "DELETE FROM firestore_ingest_suppressed_ids WHERE id = ?1",
                [fragment_id],
            )?;
            conn.execute(
                "DELETE FROM sync_tombstone_outbox WHERE entry_id = ?1",
                [fragment_id],
            )?;
        }
        self.mirror_fragment_to_entry(fragment_id)?;
        Ok(())
    }

    /// Soft-removes fragments whose legacy rows were deleted by a remote device.
    ///
    /// Called by the sync layer alongside `delete_local_entries_for_sync`, so a delete made
    /// on the phone reaches the Record. Soft, so a mistake made elsewhere does not destroy
    /// material here.
    pub fn absorb_remote_deletes(&self, ids: &[String]) -> Result<usize, rusqlite::Error> {
        if ids.is_empty() {
            return Ok(0);
        }
        let conn = self.0.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        let mut removed = 0usize;
        for id in ids {
            let n = conn.execute(
                "UPDATE fragments SET removed_at = ?1 WHERE id = ?2 AND removed_at IS NULL",
                rusqlite::params![now, id],
            )?;
            if n > 0 {
                super::record::sync_fts(&conn, id)?;
                removed += 1;
            }
        }
        Ok(removed)
    }

    /// Fragments that exist in the Record but have no legacy row yet.
    ///
    /// Lets the sync layer find everything the bridge still owes mobile — after an upgrade,
    /// or after a spell offline — without re-reading the whole Record.
    pub fn fragments_awaiting_mirror(&self, limit: i64) -> Result<Vec<String>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT f.id FROM fragments f \
             LEFT JOIN entries e ON e.id = f.id \
             WHERE e.id IS NULL AND f.removed_at IS NULL AND TRIM(f.body) <> '' \
               AND NOT EXISTS (SELECT 1 FROM firestore_ingest_suppressed_ids s WHERE s.id = f.id) \
             ORDER BY f.captured_at DESC LIMIT ?1",
        )?;
        let out: Vec<String> = stmt
            .query_map([limit], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn db() -> Db {
        Db::open(PathBuf::from(":memory:")).unwrap()
    }

    fn entry(db: &Db, id: &str) -> Option<(String, String)> {
        let conn = db.0.lock().unwrap();
        conn.query_row(
            "SELECT text, created_at FROM entries WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok()
    }

    fn count(db: &Db, sql: &str) -> i64 {
        let conn = db.0.lock().unwrap();
        conn.query_row(sql, [], |r| r.get(0)).unwrap()
    }

    // ------------------------------------------------------------ outward

    #[test]
    fn a_capture_becomes_a_legacy_row_sync_can_carry() {
        let db = db();
        let f = db.capture_fragment("dinner friday", "typed", Some("desktop")).unwrap();
        assert!(db.mirror_fragment_to_entry(&f.id).unwrap());

        let (text, created) = entry(&db, &f.id).expect("legacy row");
        assert_eq!(text, "dinner friday");
        assert_eq!(created, f.captured_at, "the legacy row keeps the capture time");
    }

    #[test]
    fn mirroring_twice_updates_one_row_rather_than_making_two() {
        let db = db();
        let f = db.capture_fragment("once", "typed", None).unwrap();
        db.mirror_fragment_to_entry(&f.id).unwrap();
        db.mirror_fragment_to_entry(&f.id).unwrap();
        db.mirror_fragment_to_entry(&f.id).unwrap();
        assert_eq!(count(&db, "SELECT COUNT(*) FROM entries"), 1);
    }

    /// The rule the legacy model most invites breaking.
    #[test]
    fn a_continuation_mirrors_as_its_own_entry_never_appended_to_the_earlier_one() {
        let db = db();
        let first = db.capture_fragment("idea: a notebook", "typed", None).unwrap();
        db.mirror_fragment_to_entry(&first.id).unwrap();
        let second = db
            .continue_fragment(&first.id, "a tag is a folder", "typed", None, false)
            .unwrap();
        db.mirror_fragment_to_entry(&second.id).unwrap();

        assert_eq!(count(&db, "SELECT COUNT(*) FROM entries"), 2);
        // The earlier row is untouched: not appended to, not re-dated.
        assert_eq!(entry(&db, &first.id).unwrap().0, "idea: a notebook");
        assert_eq!(entry(&db, &second.id).unwrap().0, "a tag is a folder");
        // And nothing wrote a v1-style in-text continuation marker.
        assert_eq!(
            count(&db, "SELECT COUNT(*) FROM entries WHERE continuation_from IS NOT NULL"),
            0
        );
    }

    #[test]
    fn a_correction_updates_the_legacy_row_and_keeps_the_history_here() {
        let db = db();
        let f = db.capture_fragment("teh wording", "typed", None).unwrap();
        db.mirror_fragment_to_entry(&f.id).unwrap();

        db.correct_fragment(&f.id, "the wording").unwrap();
        db.mirror_fragment_to_entry(&f.id).unwrap();

        assert_eq!(entry(&db, &f.id).unwrap().0, "the wording");
        assert_eq!(
            entry(&db, &f.id).unwrap().1,
            f.captured_at,
            "correcting must not re-date the legacy row either"
        );
        // The superseded wording lives where the legacy model cannot reach.
        let history = db.fragment_history(&f.id).unwrap();
        assert_eq!(history[0].body, "teh wording");
        // v1's own destructive-edit counter is not touched by a Record correction.
        assert_eq!(
            count(&db, "SELECT COALESCE(edit_count,0) FROM entries WHERE id IS NOT NULL"),
            0
        );
    }

    #[test]
    fn a_voice_fragment_waits_for_a_transcript_before_it_can_mirror() {
        let db = db();
        let f = db.capture_voice("/audio/a.wav", 6_000, None).unwrap();
        assert!(!db.mirror_fragment_to_entry(&f.id).unwrap(), "nothing to mirror yet");
        assert!(entry(&db, &f.id).is_none());

        db.record_transcript(&f.id, Ok(("something said".into(), "whisper"))).unwrap();
        assert!(db.mirror_fragment_to_entry(&f.id).unwrap());
        assert_eq!(entry(&db, &f.id).unwrap().0, "something said");
    }

    // ------------------------------------------------------------ inward

    #[test]
    fn an_entry_arriving_from_mobile_appears_in_the_record() {
        let db = db();
        db.ingest_firestore_entries(&[(
            "m1".into(),
            "captured on the phone".into(),
            "2026-09-18T10:00:00Z".into(),
        )])
        .unwrap();

        assert_eq!(db.project_entries_into_record().unwrap(), 1);

        let conn = db.0.lock().unwrap();
        let (body, method, origin): (String, String, Option<String>) = conn
            .query_row(
                "SELECT body, capture_method, capture_origin FROM fragments WHERE id = 'm1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(body, "captured on the phone");
        assert_eq!(method, "imported", "the protocol never says how it was captured");
        assert_eq!(origin.as_deref(), Some("mobile"));
    }

    #[test]
    fn a_projected_entry_is_findable() {
        let db = db();
        db.ingest_firestore_entries(&[(
            "m1".into(),
            "the radiator is cold again".into(),
            "2026-09-18T10:00:00Z".into(),
        )])
        .unwrap();
        db.project_entries_into_record().unwrap();
        assert_eq!(db.find_fragments("radiator", 10).unwrap().len(), 1);
    }

    // ------------------------------------------------------------ loops

    /// The property the whole design rests on.
    #[test]
    fn a_mirrored_capture_coming_back_through_sync_creates_nothing() {
        let db = db();
        let f = db.capture_fragment("round trip", "typed", None).unwrap();
        db.mirror_fragment_to_entry(&f.id).unwrap();

        // The phone sends it back, exactly as sync would.
        for _ in 0..3 {
            db.ingest_firestore_entries(&[(
                f.id.clone(),
                "round trip".into(),
                f.captured_at.clone(),
            )])
            .unwrap();
            db.project_entries_into_record().unwrap();
            db.mirror_fragment_to_entry(&f.id).unwrap();
        }

        assert_eq!(count(&db, "SELECT COUNT(*) FROM fragments"), 1);
        assert_eq!(count(&db, "SELECT COUNT(*) FROM entries"), 1);
        // And the capture method was not overwritten by the inbound path.
        let method: String = {
            let conn = db.0.lock().unwrap();
            conn.query_row("SELECT capture_method FROM fragments WHERE id = ?1", [&f.id], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(method, "typed", "a round trip must not downgrade provenance");
    }

    #[test]
    fn projecting_repeatedly_is_a_no_op() {
        let db = db();
        db.ingest_firestore_entries(&[("m1".into(), "once".into(), "2026-01-01T00:00:00Z".into())])
            .unwrap();
        assert_eq!(db.project_entries_into_record().unwrap(), 1);
        assert_eq!(db.project_entries_into_record().unwrap(), 0);
        assert_eq!(db.project_entries_into_record().unwrap(), 0);
        assert_eq!(count(&db, "SELECT COUNT(*) FROM fragments"), 1);
    }

    // ------------------------------------------------------------ deletes

    #[test]
    fn removing_here_tombstones_there_and_keeps_the_material() {
        let db = db();
        let f = db.capture_fragment("a passing thought", "typed", None).unwrap();
        db.mirror_fragment_to_entry(&f.id).unwrap();

        db.remove_fragment(&f.id).unwrap();

        assert!(entry(&db, &f.id).is_none(), "the legacy row goes");
        assert_eq!(count(&db, "SELECT COUNT(*) FROM sync_tombstone_outbox"), 1);
        assert_eq!(count(&db, "SELECT COUNT(*) FROM firestore_ingest_suppressed_ids"), 1);
        // Soft here: the Record does not lose material.
        let body: String = {
            let conn = db.0.lock().unwrap();
            conn.query_row("SELECT body FROM fragments WHERE id = ?1", [&f.id], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(body, "a passing thought");
        assert!(db.find_fragments("passing", 10).unwrap().is_empty(), "but it leaves Find");
    }

    #[test]
    fn a_removed_fragment_is_not_resurrected_by_the_next_pull() {
        let db = db();
        let f = db.capture_fragment("gone", "typed", None).unwrap();
        db.mirror_fragment_to_entry(&f.id).unwrap();
        db.remove_fragment(&f.id).unwrap();

        // The phone has not heard yet and sends it back.
        db.ingest_firestore_entries(&[(f.id.clone(), "gone".into(), f.captured_at.clone())])
            .unwrap();
        db.project_entries_into_record().unwrap();

        assert!(entry(&db, &f.id).is_none(), "suppression holds");
        // Mirroring must not undo the removal either.
        assert!(!db.mirror_fragment_to_entry(&f.id).unwrap());
    }

    #[test]
    fn undo_puts_it_back_and_lets_it_mirror_again() {
        let db = db();
        let f = db.capture_fragment("kept after all", "typed", None).unwrap();
        db.mirror_fragment_to_entry(&f.id).unwrap();
        db.remove_fragment(&f.id).unwrap();

        db.restore_fragment(&f.id).unwrap();

        assert_eq!(entry(&db, &f.id).unwrap().0, "kept after all");
        assert_eq!(count(&db, "SELECT COUNT(*) FROM sync_tombstone_outbox"), 0);
        assert_eq!(count(&db, "SELECT COUNT(*) FROM firestore_ingest_suppressed_ids"), 0);
        assert_eq!(db.find_fragments("kept", 10).unwrap().len(), 1);
    }

    #[test]
    fn a_delete_from_another_device_softly_removes_it_here() {
        let db = db();
        db.ingest_firestore_entries(&[("m1".into(), "from the phone".into(), "2026-01-01T00:00:00Z".into())])
            .unwrap();
        db.project_entries_into_record().unwrap();

        assert_eq!(db.absorb_remote_deletes(&["m1".to_string()]).unwrap(), 1);

        assert!(db.find_fragments("phone", 10).unwrap().is_empty());
        let body: String = {
            let conn = db.0.lock().unwrap();
            conn.query_row("SELECT body FROM fragments WHERE id = 'm1'", [], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(body, "from the phone", "a remote delete does not destroy material here");
    }

    // ------------------------------------------------------------ catch-up

    #[test]
    fn the_bridge_can_find_what_it_still_owes() {
        let db = db();
        let a = db.capture_fragment("one", "typed", None).unwrap();
        let b = db.capture_fragment("two", "typed", None).unwrap();
        db.mirror_fragment_to_entry(&a.id).unwrap();

        let owed = db.fragments_awaiting_mirror(100).unwrap();
        assert_eq!(owed, vec![b.id.clone()]);

        db.mirror_fragment_to_entry(&b.id).unwrap();
        assert!(db.fragments_awaiting_mirror(100).unwrap().is_empty());
    }

    #[test]
    fn removed_and_empty_fragments_are_never_owed_to_the_legacy_table() {
        let db = db();
        let removed = db.capture_fragment("will go", "typed", None).unwrap();
        db.remove_fragment(&removed.id).unwrap();
        let silent = db.capture_voice("/audio/a.wav", 1000, None).unwrap();

        let owed = db.fragments_awaiting_mirror(100).unwrap();
        assert!(!owed.contains(&removed.id));
        assert!(!owed.contains(&silent.id));
    }
}
