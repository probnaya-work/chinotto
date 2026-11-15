//! Structured SQLite boundary for additive Record metadata. No network runs under this lock.
use super::Db;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncRevision {
    pub body: String,
    pub superseded_at: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncLink {
    pub continues_id: String,
    pub linked_at: String,
    pub origin: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncSnapshot {
    pub id: String,
    pub body: String,
    pub captured_at: String,
    pub history: Vec<SyncRevision>,
    pub continuation: Option<SyncLink>,
    pub state: Option<String>,
    pub generation: i64,
    pub dirty: bool,
    pub resolved_at: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncCommit {
    pub snapshot: SyncSnapshot,
    pub state: String,
    pub wire: serde_json::Value,
    pub body: String,
    pub corrected_at: Option<String>,
    pub revisions: Vec<SyncRevision>,
    pub conflict: Option<String>,
}
fn snapshot(conn: &Connection, id: &str) -> Result<Option<SyncSnapshot>, rusqlite::Error> {
    let row = conn
        .query_row(
            "SELECT f.body, f.captured_at, s.state, s.generation, s.dirty, w.resolved_at
         FROM fragments f JOIN record_sync_state s ON s.fragment_id = f.id
         LEFT JOIN wording_conflicts w ON w.fragment_id = f.id
         WHERE f.id = ?1 AND f.removed_at IS NULL AND TRIM(f.body) <> ''
           AND NOT EXISTS (SELECT 1 FROM firestore_ingest_suppressed_ids x WHERE x.id = f.id)",
            [id],
            |r| {
                Ok(SyncSnapshot {
                    id: id.to_string(),
                    body: r.get(0)?,
                    captured_at: r.get(1)?,
                    state: r.get(2)?,
                    generation: r.get(3)?,
                    dirty: r.get::<_, i64>(4)? != 0,
                    resolved_at: r.get(5)?,
                    history: vec![],
                    continuation: None,
                })
            },
        )
        .optional()?;
    let Some(mut row) = row else { return Ok(None) };
    let mut stmt = conn.prepare(
        "SELECT body, superseded_at FROM fragment_revisions WHERE fragment_id = ?1 ORDER BY revision_index")?;
    row.history = stmt
        .query_map([id], |r| {
            Ok(SyncRevision {
                body: r.get(0)?,
                superseded_at: r.get(1)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    row.continuation = conn.query_row(
        "SELECT continues_id, linked_at, origin FROM continuations WHERE fragment_id = ?1
         UNION ALL SELECT continues_id, linked_at, origin FROM record_sync_links WHERE fragment_id = ?1 LIMIT 1",
        [id], |r| Ok(SyncLink { continues_id: r.get(0)?, linked_at: r.get(1)?, origin: r.get(2)? }),
    ).optional()?;
    Ok(Some(row))
}
impl Db {
    pub fn record_sync_snapshot(&self, id: &str) -> Result<Option<SyncSnapshot>, rusqlite::Error> {
        snapshot(&self.0.lock().unwrap(), id)
    }
    pub fn pending_record_sync_ids(&self, limit: i64) -> Result<Vec<String>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT s.fragment_id FROM record_sync_state s JOIN fragments f ON f.id = s.fragment_id
             WHERE s.dirty = 1 AND f.removed_at IS NULL AND TRIM(f.body) <> ''
             ORDER BY s.error IS NOT NULL,
               CASE WHEN s.error IS NULL THEN s.touched_at END DESC, s.touched_at, s.rowid LIMIT ?1")?;
        let rows = stmt
            .query_map([limit.clamp(1, 500)], |r| r.get(0))?
            .collect();
        rows
    }
    pub fn acknowledge_record_sync(
        &self,
        id: &str,
        generation: i64,
    ) -> Result<(), rusqlite::Error> {
        self.0.lock().unwrap().execute(
            "UPDATE record_sync_state SET dirty = 0, error = NULL WHERE fragment_id = ?1 AND generation = ?2",
            rusqlite::params![id, generation])?;
        Ok(())
    }
    pub fn record_sync_error(&self, id: &str, message: &str) -> Result<(), rusqlite::Error> {
        let message: String = message.chars().take(200).collect();
        self.0.lock().unwrap().execute(
            "UPDATE record_sync_state SET error = ?1, touched_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE fragment_id = ?2",
            rusqlite::params![message, id])?;
        Ok(())
    }
    pub fn failed_record_sync_count(&self) -> Result<i64, rusqlite::Error> {
        self.0.lock().unwrap().query_row(
            "SELECT COUNT(*) FROM record_sync_state s JOIN fragments f ON f.id = s.fragment_id
             WHERE s.dirty = 1 AND s.error IS NOT NULL AND f.removed_at IS NULL",
            [],
            |r| r.get(0),
        )
    }
    pub fn activate_record_sync_user(&self, uid: &str) -> Result<(), rusqlite::Error> {
        let mut conn = self.0.lock().unwrap();
        let tx = conn.transaction()?;
        let last: Option<String> = tx
            .query_row(
                "SELECT uid FROM record_sync_account WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if last.as_deref() != Some(uid) {
            tx.execute("UPDATE record_sync_state SET dirty = 1, error = NULL", [])?;
            tx.execute(
                "INSERT INTO record_sync_account (id, uid) VALUES (1, ?1)
                ON CONFLICT(id) DO UPDATE SET uid = excluded.uid",
                [uid],
            )?;
        }
        tx.commit()
    }
    pub fn pending_record_sync_count(&self) -> Result<i64, rusqlite::Error> {
        self.0.lock().unwrap().query_row(
            "SELECT COUNT(*) FROM record_sync_state s JOIN fragments f ON f.id = s.fragment_id
             WHERE s.dirty = 1 AND f.removed_at IS NULL AND TRIM(f.body) <> ''",
            [],
            |r| r.get(0),
        )
    }
    pub fn ensure_record_sync_fragment(
        &self,
        id: &str,
        body: &str,
        at: &str,
    ) -> Result<bool, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let removed: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM firestore_ingest_suppressed_ids WHERE id = ?1)
               OR EXISTS(SELECT 1 FROM fragments WHERE id = ?1 AND removed_at IS NOT NULL)",
            [id],
            |r| r.get(0),
        )?;
        if removed || body.trim().is_empty() || chrono::DateTime::parse_from_rfc3339(at).is_err() {
            return Ok(false);
        }
        conn.execute(
            "INSERT OR IGNORE INTO fragments (id, body, captured_at, capture_method, capture_origin, legacy_entry_id)
             VALUES (?1, ?2, ?3, 'imported', 'sync', ?1)", rusqlite::params![id, body, at])?;
        Ok(true)
    }
    pub fn commit_record_sync(&self, commit: &SyncCommit) -> Result<bool, rusqlite::Error> {
        let mut conn = self.0.lock().unwrap();
        let tx = conn.transaction()?;
        let before = snapshot(&tx, &commit.snapshot.id)?;
        let Some(before) = before else {
            return Ok(false);
        };
        if before != commit.snapshot {
            return Ok(false);
        }
        if commit.body.trim().is_empty() || commit.state.len() > 1_000_000 {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let state: serde_json::Value =
            serde_json::from_str(&commit.state).map_err(|_| rusqlite::Error::InvalidQuery)?;
        let selected = state["selected"]
            .as_str()
            .ok_or(rusqlite::Error::InvalidQuery)?;
        if state["wire"]["wordings"][selected]["body"].as_str() != Some(commit.body.as_str())
            || state["revisionCount"].as_u64()
                != Some((before.history.len() + commit.revisions.len()) as u64)
        {
            return Err(rusqlite::Error::InvalidQuery);
        }
        if before.state.as_deref() == Some(commit.state.as_str())
            && commit.revisions.is_empty()
            && commit.body == before.body
        {
            return Ok(true);
        }
        for (i, r) in commit.revisions.iter().enumerate() {
            if chrono::DateTime::parse_from_rfc3339(&r.superseded_at).is_err() {
                return Err(rusqlite::Error::InvalidQuery);
            }
            tx.execute(
                "INSERT INTO fragment_revisions (fragment_id, body, superseded_at, revision_index)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![
                    before.id,
                    r.body,
                    r.superseded_at,
                    (before.history.len() + i) as i64
                ],
            )?;
        }
        tx.execute(
            "UPDATE fragments SET body = ?1, corrected_at = ?2, correction_count = ?3 WHERE id = ?4",
            rusqlite::params![commit.body, commit.corrected_at, (before.history.len() + commit.revisions.len()) as i64, before.id])?;
        tx.execute(
            "UPDATE entries SET text = ?1 WHERE id = ?2",
            rusqlite::params![commit.body.trim(), before.id],
        )?;
        if let Some(remote) = &commit.conflict {
            tx.execute(
                "INSERT INTO wording_conflicts (fragment_id, remote_text, local_text, noticed_at, shows)
                 VALUES (?1, ?2, ?3, ?4, 'local') ON CONFLICT(fragment_id) DO UPDATE SET
                 remote_text = excluded.remote_text, local_text = excluded.local_text,
                 noticed_at = excluded.noticed_at, resolved_at = NULL",
                rusqlite::params![before.id, remote, commit.body, chrono::Utc::now().to_rfc3339()])?;
        } else {
            tx.execute(
                "DELETE FROM wording_conflicts WHERE fragment_id = ?1",
                [&before.id],
            )?;
        }
        if !commit.wire["continuation"].is_null() {
            let link: SyncLink = serde_json::from_value(commit.wire["continuation"].clone())
                .map_err(|_| rusqlite::Error::InvalidQuery)?;
            let removed: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM firestore_ingest_suppressed_ids WHERE id = ?1)
                   OR EXISTS(SELECT 1 FROM fragments WHERE id = ?1 AND removed_at IS NOT NULL)",
                [&link.continues_id],
                |r| r.get(0),
            )?;
            if !removed
                && link.continues_id != before.id
                && ["explicit", "accepted_suggestion"].contains(&link.origin.as_str())
            {
                tx.execute(
                    "INSERT OR IGNORE INTO record_sync_links (fragment_id, continues_id, linked_at, origin)
                     VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![before.id, link.continues_id, link.linked_at, link.origin])?;
            }
        }
        tx.execute(
            "UPDATE record_sync_state SET state = ?1, dirty = ?2, error = NULL WHERE fragment_id = ?3",
            rusqlite::params![commit.state, before.dirty, before.id])?;
        super::record::sync_fts(&tx, &before.id)?;
        tx.commit()?;
        Ok(true)
    }
    pub fn apply_pending_record_sync_links(&self) -> Result<usize, rusqlite::Error> {
        let mut conn = self.0.lock().unwrap();
        let tx = conn.transaction()?;
        let links: Vec<(String, SyncLink)> = {
            let mut stmt = tx.prepare(
                "SELECT l.fragment_id, l.continues_id, l.linked_at, l.origin FROM record_sync_links l
                 JOIN fragments f ON f.id = l.fragment_id AND f.removed_at IS NULL
                 JOIN fragments p ON p.id = l.continues_id AND p.removed_at IS NULL
                 WHERE NOT EXISTS (SELECT 1 FROM continuations c WHERE c.fragment_id = l.fragment_id)
                   AND NOT EXISTS (SELECT 1 FROM firestore_ingest_suppressed_ids s
                     WHERE s.id = l.fragment_id OR s.id = l.continues_id)")?;
            let rows = stmt
                .query_map([], |r| {
                    Ok((
                        r.get(0)?,
                        SyncLink {
                            continues_id: r.get(1)?,
                            linked_at: r.get(2)?,
                            origin: r.get(3)?,
                        },
                    ))
                })?
                .collect::<Result<_, _>>()?;
            rows
        };
        let mut changed = 0;
        for (id, link) in links {
            let cycle: bool = tx.query_row(
                "WITH RECURSIVE ancestors(id) AS (
                   SELECT ?1 UNION SELECT c.continues_id FROM continuations c JOIN ancestors a ON c.fragment_id = a.id
                 ) SELECT EXISTS(SELECT 1 FROM ancestors WHERE id = ?2)",
                rusqlite::params![link.continues_id, id], |r| r.get(0))?;
            if cycle {
                continue;
            }
            changed += tx.execute(
                "INSERT OR IGNORE INTO continuations (fragment_id, continues_id, linked_at, origin)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![id, link.continues_id, link.linked_at, link.origin],
            )?;
        }
        if changed > 0 {
            super::migrate::rebuild_line_index(&tx)?
        }
        tx.commit()?;
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    fn fixture() -> serde_json::Value {
        serde_json::from_str(include_str!("record_sync_fixture.json")).unwrap()
    }
    fn db() -> Db {
        Db::open(PathBuf::from(":memory:")).unwrap()
    }
    #[test]
    fn cross_platform_fixture() {
        let db = db();
        let fixture = fixture();
        let initial: SyncCommit = serde_json::from_value(fixture["initial"].clone()).unwrap();
        assert!(db
            .ensure_record_sync_fragment(
                &initial.snapshot.id,
                &initial.snapshot.body,
                &initial.snapshot.captured_at
            )
            .unwrap());
        assert!(db.commit_record_sync(&initial).unwrap());
        let receive: SyncCommit = serde_json::from_value(fixture["receive"].clone()).unwrap();
        assert!(db.commit_record_sync(&receive).unwrap());
        assert!(
            !db.commit_record_sync(&receive).unwrap(),
            "an old CAS token cannot be replayed"
        );
        let parent = db.record_sync_snapshot("fixture-moment").unwrap().unwrap();
        assert_eq!(parent.body, "phone correction");
        assert_eq!(parent.history.len(), 1);
        assert_eq!(parent.history[0].body, "original wording");
        assert_eq!(parent.captured_at, "2026-11-01T10:00:00.000Z");
        let child = db
            .continue_fragment(
                "fixture-moment",
                "native continuation",
                "typed",
                Some("desktop"),
                false,
            )
            .unwrap();
        let snapshot = db.record_sync_snapshot(&child.id).unwrap().unwrap();
        assert_eq!(
            snapshot.continuation.unwrap().continues_id,
            "fixture-moment"
        );
        if let Ok(path) = std::env::var("CHINOTTO_NATIVE_SYNC_FIXTURE") {
            std::fs::write(
                path,
                serde_json::to_vec(&serde_json::json!({
                    "parent": parent,
                    "child": db.record_sync_snapshot(&child.id).unwrap().unwrap(),
                }))
                .unwrap(),
            )
            .unwrap();
        }
    }
    #[test]
    fn upload_acknowledgement_cannot_consume_a_new_local_correction() {
        let db = db();
        db.ensure_record_sync_fragment("first", "original", "2026-11-01T10:00:00.000Z")
            .unwrap();
        let before = db.record_sync_snapshot("first").unwrap().unwrap();
        db.correct_fragment("first", "written while sending")
            .unwrap();
        db.acknowledge_record_sync("first", before.generation)
            .unwrap();
        assert!(db
            .pending_record_sync_ids(25)
            .unwrap()
            .contains(&"first".to_string()));
    }
    #[test]
    fn pending_link_waits_for_its_real_parent_and_does_not_create_a_cycle() {
        let db = db();
        db.ensure_record_sync_fragment("child", "later", "2026-11-02T10:00:00.000Z")
            .unwrap();
        db.0.lock().unwrap().execute(
            "INSERT INTO record_sync_links VALUES ('child', 'parent', '2026-11-02T10:00:00.000Z', 'explicit')", []).unwrap();
        assert_eq!(db.apply_pending_record_sync_links().unwrap(), 0);
        db.ensure_record_sync_fragment("parent", "earlier", "2026-11-01T10:00:00.000Z")
            .unwrap();
        assert_eq!(db.apply_pending_record_sync_links().unwrap(), 1);
        assert_eq!(db.line_for("child").unwrap().len(), 2);
        db.0.lock().unwrap().execute(
            "INSERT INTO record_sync_links VALUES ('parent', 'child', '2026-11-03T10:00:00.000Z', 'explicit')", []).unwrap();
        assert_eq!(db.apply_pending_record_sync_links().unwrap(), 0);
    }
    #[test]
    fn a_removed_record_erases_sync_material_and_rejects_delayed_metadata() {
        let db = db();
        let fixture = fixture();
        let initial: SyncCommit = serde_json::from_value(fixture["initial"].clone()).unwrap();
        db.ensure_record_sync_fragment(
            &initial.snapshot.id,
            &initial.snapshot.body,
            &initial.snapshot.captured_at,
        )
        .unwrap();
        db.commit_record_sync(&initial).unwrap();
        db.remove_fragment("fixture-moment").unwrap();
        db.erase_voice_content("fixture-moment", "2099-01-01T00:00:00Z")
            .unwrap();
        assert!(db.record_sync_snapshot("fixture-moment").unwrap().is_none());
        assert!(!db
            .ensure_record_sync_fragment(
                "fixture-moment",
                "original wording",
                &initial.snapshot.captured_at
            )
            .unwrap());
        assert!(!db.commit_record_sync(&initial).unwrap());
        let count: i64 =
            db.0.lock()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM record_sync_state WHERE fragment_id = 'fixture-moment'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
        assert_eq!(count, 0);
    }
    #[test]
    fn failed_sends_rotate_behind_untried_changes() {
        let db = db();
        for id in ["old", "new"] {
            db.ensure_record_sync_fragment(id, id, "2026-11-01T10:00:00.000Z")
                .unwrap();
        }
        db.record_sync_error("old", "failed").unwrap();
        assert_eq!(db.pending_record_sync_ids(1).unwrap(), vec!["new"]);
        db.record_sync_error("new", "failed").unwrap();
        assert_eq!(db.failed_record_sync_count().unwrap(), 2);
        db.0.lock().unwrap().execute(
            "UPDATE record_sync_state SET touched_at = '2099-01-01T00:00:00Z' WHERE fragment_id = 'new'", []).unwrap();
        assert_eq!(db.pending_record_sync_ids(1).unwrap(), vec!["old"]);
    }
    #[test]
    fn metadata_is_requeued_for_a_new_account() {
        let db = db();
        db.ensure_record_sync_fragment("one", "wording", "2026-11-01T10:00:00.000Z")
            .unwrap();
        db.activate_record_sync_user("a").unwrap();
        let generation = db.record_sync_snapshot("one").unwrap().unwrap().generation;
        db.acknowledge_record_sync("one", generation).unwrap();
        db.activate_record_sync_user("a").unwrap();
        assert_eq!(db.pending_record_sync_count().unwrap(), 0);
        db.activate_record_sync_user("b").unwrap();
        assert_eq!(db.pending_record_sync_count().unwrap(), 1);
    }
}
