//! Versioned schema migrations, keyed on `PRAGMA user_version`.
//!
//! v1 had no migration framework: `schema.sql` was replayed with CREATE TABLE IF NOT EXISTS
//! and columns were bolted on by guarded `ALTER TABLE` calls. That is fine for adding a
//! column and useless for a change of model, so the Record gets a real ladder.
//!
//! Rules this module holds to:
//!   * Migrations are forward-only, idempotent, and run inside one transaction each.
//!   * v1 tables are READ, never dropped here. The app stops reading them long before it
//!     stops having them, so a bad migration is recoverable by reverting the binary.
//!   * Nothing is invented. Where v1 genuinely does not know something (how a fragment was
//!     captured, what an entry said before it was edited in place), the migration records
//!     that it does not know instead of guessing a plausible value.

use rusqlite::{Connection, Transaction};
use std::path::Path;

/// Schema version this binary expects. Bump when adding a migration below.
pub const TARGET_VERSION: i32 = 5;

pub fn current_version(conn: &Connection) -> Result<i32, rusqlite::Error> {
    conn.query_row("PRAGMA user_version", [], |r| r.get(0))
}

fn set_version(tx: &Transaction, v: i32) -> Result<(), rusqlite::Error> {
    // PRAGMA does not accept bound parameters.
    tx.execute_batch(&format!("PRAGMA user_version = {v}"))
}

/// Runs every migration needed to reach [`TARGET_VERSION`].
///
/// `archive_dir` is where material that is leaving the product is written before it goes,
/// so removing a concept never silently destroys something a person typed.
pub fn run(conn: &mut Connection, archive_dir: Option<&Path>) -> Result<(), rusqlite::Error> {
    let from = current_version(conn)?;
    if from >= TARGET_VERSION {
        return Ok(());
    }

    if from < 2 {
        let tx = conn.transaction()?;
        migrate_to_v2(&tx, archive_dir)?;
        set_version(&tx, 2)?;
        tx.commit()?;
    }

    if from < 3 {
        let tx = conn.transaction()?;
        migrate_to_v3(&tx)?;
        set_version(&tx, 3)?;
        tx.commit()?;
    }

    if from < 4 {
        let tx = conn.transaction()?;
        migrate_to_v4(&tx)?;
        set_version(&tx, 4)?;
        tx.commit()?;
    }

    if from < 5 {
        let tx = conn.transaction()?;
        migrate_to_v5(&tx)?;
        set_version(&tx, 5)?;
        tx.commit()?;
    }

    Ok(())
}

/// v1 (entries / spaces / pins / themes) -> v2 (the Record).
///
/// Copies forward; drops nothing. The v1 tables are left exactly as they are.
fn migrate_to_v2(tx: &Transaction, archive_dir: Option<&Path>) -> Result<(), rusqlite::Error> {
    tx.execute_batch(include_str!("schema_v2.sql"))?;

    if table_exists(tx, "entries")? {
        copy_entries_to_fragments(tx)?;
    }
    if table_exists(tx, "pinned_entries")? {
        copy_pins_to_holds(tx)?;
    }
    if let Some(dir) = archive_dir {
        archive_themes(tx, dir)?;
    }

    rebuild_line_index(tx)?;
    Ok(())
}

/// v3: somewhere for Find to keep meaning vectors.
///
/// Purely additive and purely derived — the table can be dropped and rebuilt from the
/// fragments themselves at any time, which is why it carries `computed_at` and the model
/// name: a cache whose provenance is unknown is a cache you cannot safely invalidate.
fn migrate_to_v3(tx: &Transaction) -> Result<(), rusqlite::Error> {
    tx.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS fragment_embeddings (
          fragment_id  TEXT PRIMARY KEY REFERENCES fragments(id) ON DELETE CASCADE,
          embedding    BLOB NOT NULL,
          model        TEXT NOT NULL,
          computed_at  TEXT NOT NULL,
          -- The wording the vector was computed from, so a correction invalidates it.
          body_hash    TEXT NOT NULL
        );
        "#,
    )?;

    // v1 embedded entries, and fragments carry the same ids, so the work already done is
    // reusable rather than recomputed for every existing fragment.
    if table_exists(tx, "entry_embeddings")? {
        tx.execute_batch(
            r#"
            INSERT OR IGNORE INTO fragment_embeddings (fragment_id, embedding, model, computed_at, body_hash)
            SELECT e.entry_id, e.embedding, 'AllMiniLML6V2', '', ''
            FROM entry_embeddings e
            JOIN fragments f ON f.id = e.entry_id;
            "#,
        )?;
    }
    Ok(())
}

/// v4: separates what arrived from what was fetched about it.
///
/// v2 put `title`, `url_canonical` and `extraction` on `encounters`, and the machine
/// transcript on `voice_captures` — canonical tables. That was wrong: all four are derived,
/// can fail, and can change on a later attempt, so they cannot sit beside the URL that was
/// actually received or the audio that was actually recorded. This moves them out.
///
/// Additive: the old columns are left in place and simply stop being read. Nothing is
/// dropped here.
fn migrate_to_v4(tx: &Transaction) -> Result<(), rusqlite::Error> {
    tx.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS encounter_enrichment (
          encounter_id   INTEGER PRIMARY KEY REFERENCES encounters(id) ON DELETE CASCADE,
          url_canonical  TEXT,
          domain         TEXT,
          title          TEXT,
          site_name      TEXT,
          extraction     TEXT,
          state          TEXT NOT NULL DEFAULT 'pending' CHECK (state IN ('pending','ok','failed')),
          attempted_at   TEXT,
          fetched_at     TEXT,
          failure        TEXT
        );

        CREATE TABLE IF NOT EXISTS voice_transcripts (
          fragment_id        TEXT PRIMARY KEY REFERENCES fragments(id) ON DELETE CASCADE,
          machine_transcript TEXT,
          state              TEXT NOT NULL DEFAULT 'pending' CHECK (state IN ('pending','ok','failed')),
          model              TEXT,
          transcribed_at     TEXT,
          failure            TEXT
        );
        "#,
    )?;

    // Columns the canonical tables gained.
    for (table, column, ddl) in [
        ("encounters", "url_key", "ALTER TABLE encounters ADD COLUMN url_key TEXT NOT NULL DEFAULT ''"),
        ("voice_captures", "recorded_at", "ALTER TABLE voice_captures ADD COLUMN recorded_at TEXT NOT NULL DEFAULT ''"),
        ("voice_captures", "audio_missing", "ALTER TABLE voice_captures ADD COLUMN audio_missing INTEGER NOT NULL DEFAULT 0"),
    ] {
        if table_exists(tx, table)? && !column_exists(tx, table, column)? {
            tx.execute_batch(ddl)?;
        }
    }

    // Carry any enrichment that was already recorded into the derived tables. In practice
    // these tables have never been written to, but a migration should not assume that.
    if table_exists(tx, "encounters")? && column_exists(tx, "encounters", "title")? {
        tx.execute_batch(
            "INSERT OR IGNORE INTO encounter_enrichment                (encounter_id, url_canonical, domain, title, extraction, state, fetched_at)              SELECT id, url_canonical, domain, title, extraction,                     CASE WHEN title IS NOT NULL OR url_canonical IS NOT NULL THEN 'ok' ELSE 'pending' END,                     metadata_fetched_at              FROM encounters;",
        )?;
        // An exact URL is still a durable key, just a stricter one than the normalised form
        // Rust computes at capture. Better a narrow key than a fabricated one.
        tx.execute_batch("UPDATE encounters SET url_key = url_raw WHERE url_key = '';")?;
    }

    if table_exists(tx, "voice_captures")? && column_exists(tx, "voice_captures", "machine_transcript")? {
        tx.execute_batch(
            "INSERT OR IGNORE INTO voice_transcripts (fragment_id, machine_transcript, state, transcribed_at)              SELECT fragment_id, machine_transcript, COALESCE(transcript_state, 'pending'), transcribed_at              FROM voice_captures;",
        )?;
        tx.execute_batch(
            "UPDATE voice_captures SET recorded_at = (                SELECT f.captured_at FROM fragments f WHERE f.id = voice_captures.fragment_id)              WHERE recorded_at = '';",
        )?;
    }

    Ok(())
}

/// v5: a Return says which act caused it.
///
/// `schema_v2.sql` gained `return_evidence.related_id`, but that file only runs when the
/// Record is first created — `CREATE TABLE IF NOT EXISTS` does not alter a table that is
/// already there. A database built by an earlier build of this branch is already at v4, so
/// `run()` returns early and the column never arrives; `select_return` then fails on every
/// read with "no such column", and the person loses Returns entirely.
///
/// Additive and nullable: existing evidence rows simply have no cause recorded, which is
/// the truth about them — they were written before the product asked for one.
fn migrate_to_v5(tx: &Transaction) -> Result<(), rusqlite::Error> {
    if table_exists(tx, "return_evidence")? && !column_exists(tx, "return_evidence", "related_id")? {
        tx.execute_batch(
            "ALTER TABLE return_evidence ADD COLUMN related_id TEXT REFERENCES fragments(id) ON DELETE SET NULL",
        )?;
    }
    Ok(())
}

fn table_exists(tx: &Transaction, name: &str) -> Result<bool, rusqlite::Error> {
    let n: i32 = tx.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [name],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

fn column_exists(tx: &Transaction, table: &str, column: &str) -> Result<bool, rusqlite::Error> {
    let mut stmt = tx.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Every v1 entry becomes exactly one fragment, text byte-for-byte unchanged.
///
/// Deliberately NOT done here:
///   * `continuation_from` is a UTF-16 offset inside one entry's own text. v2's Continue is
///     a relationship between two separately dated fragments. They share a word and nothing
///     else, so the offset is carried across as provenance and creates no Line. Promoting it
///     to an edge would be inventing a continuation the person never made.
///   * `edit_count` is not carried into `correction_count`. v1 overwrote text in place, so
///     the earlier wordings are already gone; claiming N corrections while holding zero prior
///     wordings would make the history lie. It goes to `legacy_edit_count` instead.
///   * `capture_method` is 'imported', not 'typed'. v1 did not record how a thing was
///     captured and the interface should not assert one.
fn copy_entries_to_fragments(tx: &Transaction) -> Result<(), rusqlite::Error> {
    let has_cont = column_exists(tx, "entries", "continuation_from")?;
    let has_edit = column_exists(tx, "entries", "edit_count")?;
    let has_updated = column_exists(tx, "entries", "updated_at")?;

    let cont_from = if has_cont { "continuation_from" } else { "NULL" };
    let cont_at = if has_cont { "continuation_at" } else { "NULL" };
    let edit = if has_edit { "COALESCE(edit_count, 0)" } else { "0" };
    // A corrected-at only means something if v1 actually recorded an edit.
    let corrected = if has_edit && has_updated {
        "CASE WHEN COALESCE(edit_count, 0) > 0 AND updated_at <> '' THEN updated_at ELSE NULL END"
    } else {
        "NULL"
    };

    tx.execute_batch(&format!(
        r#"
        INSERT OR IGNORE INTO fragments (
            id, body, captured_at, capture_method, capture_origin,
            corrected_at, correction_count, removed_at,
            legacy_entry_id, legacy_continuation_offset, legacy_continuation_at, legacy_edit_count
        )
        SELECT
            id,
            text,
            created_at,
            'imported',
            'legacy',
            {corrected},
            0,
            NULL,
            id,
            {cont_from},
            {cont_at},
            {edit}
        FROM entries;
        "#
    ))?;

    Ok(())
}

/// Pins carried a real intention — "keep this where I can see it" — even though the pin
/// collection itself is leaving. That intention becomes a hold; the pin time is preserved
/// so a Return can still say how long something has been kept present.
fn copy_pins_to_holds(tx: &Transaction) -> Result<(), rusqlite::Error> {
    tx.execute_batch(
        r#"
        INSERT OR IGNORE INTO holds (fragment_id, held_at, released_at)
        SELECT p.entry_id, p.pinned_at, NULL
        FROM pinned_entries p
        JOIN fragments f ON f.id = p.entry_id;
        "#,
    )?;
    Ok(())
}

/// Themes are leaving the product. The labels were typed by a person, so they are written
/// out as readable JSON next to the database before the concept goes. The tables themselves
/// are dropped in a later migration, once no code reads them.
fn archive_themes(tx: &Transaction, archive_dir: &Path) -> Result<(), rusqlite::Error> {
    if !table_exists(tx, "user_themes")? {
        return Ok(());
    }

    let mut user_themes = Vec::new();
    {
        let mut stmt =
            tx.prepare("SELECT id, label, sort_order, created_at FROM user_themes ORDER BY sort_order")?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            user_themes.push(serde_json::json!({
                "id": r.get::<_, String>(0)?,
                "label": r.get::<_, String>(1)?,
                "sort_order": r.get::<_, i64>(2)?,
                "created_at": r.get::<_, String>(3)?,
            }));
        }
    }

    let mut assignments = Vec::new();
    if table_exists(tx, "entry_themes")? {
        let mut stmt = tx.prepare(
            "SELECT entry_id, theme_id, confidence, source, locked, classified_at FROM entry_themes",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            assignments.push(serde_json::json!({
                "entry_id": r.get::<_, String>(0)?,
                "theme_id": r.get::<_, String>(1)?,
                "confidence": r.get::<_, f64>(2)?,
                "source": r.get::<_, String>(3)?,
                "locked": r.get::<_, i64>(4)? != 0,
                "classified_at": r.get::<_, String>(5)?,
            }));
        }
    }

    if user_themes.is_empty() && assignments.is_empty() {
        return Ok(());
    }

    let doc = serde_json::json!({
        "archived_at": chrono::Utc::now().to_rfc3339(),
        "note": "Themes were removed from Chinotto in v2. Kept here so nothing typed is lost.",
        "user_themes": user_themes,
        "entry_themes": assignments,
    });

    // An archive that cannot be written must not take the migration down with it: the
    // material still exists in the v1 tables, which this migration does not drop.
    let _ = std::fs::create_dir_all(archive_dir);
    let path = archive_dir.join("themes-archive.json");
    if let Ok(text) = serde_json::to_string_pretty(&doc) {
        let _ = std::fs::write(&path, text);
    }

    Ok(())
}

/// Recomputes `line_index` from `continuations`. Pure cache: safe to run at any time.
///
/// Walks each chain from its root. `continuations.fragment_id` is a PRIMARY KEY so a
/// fragment continues at most one earlier fragment, which makes every line a chain; the
/// visited set is belt-and-braces against a cycle introduced by a bad write.
pub fn rebuild_line_index(tx: &Transaction) -> Result<(), rusqlite::Error> {
    tx.execute_batch("DELETE FROM line_index")?;

    // child -> parent
    let mut edges: Vec<(String, String)> = Vec::new();
    {
        let mut stmt = tx.prepare("SELECT fragment_id, continues_id FROM continuations")?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            edges.push((r.get(0)?, r.get(1)?));
        }
    }
    if edges.is_empty() {
        return Ok(());
    }

    use std::collections::{HashMap, HashSet};
    let parent: HashMap<&str, &str> = edges
        .iter()
        .map(|(c, p)| (c.as_str(), p.as_str()))
        .collect();
    let mut children: HashMap<&str, Vec<&str>> = HashMap::new();
    for (c, p) in &edges {
        children.entry(p.as_str()).or_default().push(c.as_str());
    }
    // Deterministic order when a moment was continued more than once.
    for v in children.values_mut() {
        v.sort_unstable();
    }

    let roots: HashSet<&str> = edges
        .iter()
        .map(|(_, p)| p.as_str())
        .filter(|p| !parent.contains_key(p))
        .collect();

    let mut insert = tx.prepare(
        "INSERT OR REPLACE INTO line_index (fragment_id, root_id, position, line_length) \
         VALUES (?1, ?2, ?3, ?4)",
    )?;

    for root in roots {
        // Depth-first so a line reads in the order its moments were added.
        let mut order: Vec<&str> = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            if !seen.insert(node) {
                continue;
            }
            order.push(node);
            if let Some(kids) = children.get(node) {
                for kid in kids.iter().rev() {
                    stack.push(kid);
                }
            }
        }

        let len = order.len() as i64;
        for (i, node) in order.iter().enumerate() {
            insert.execute(rusqlite::params![node, root, i as i64, len])?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    /// A database shaped like v1 at its final state, before the Record existed.
    fn v1_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE entries (
              id TEXT PRIMARY KEY,
              text TEXT NOT NULL,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL DEFAULT '',
              edit_count INTEGER DEFAULT 0,
              open_count INTEGER DEFAULT 0,
              space_id TEXT,
              continuation_from INTEGER,
              continuation_at TEXT
            );
            CREATE TABLE spaces (id TEXT PRIMARY KEY, label TEXT NOT NULL, sort_order INTEGER NOT NULL);
            CREATE TABLE pinned_entries (entry_id TEXT PRIMARY KEY, pinned_at TEXT NOT NULL);
            CREATE TABLE user_themes (
              id TEXT PRIMARY KEY, label TEXT NOT NULL, keywords TEXT NOT NULL,
              sort_order INTEGER NOT NULL, created_at TEXT NOT NULL
            );
            CREATE TABLE entry_themes (
              entry_id TEXT PRIMARY KEY, theme_id TEXT NOT NULL, confidence REAL NOT NULL,
              source TEXT NOT NULL, locked INTEGER NOT NULL DEFAULT 0, classified_at TEXT NOT NULL
            );
            "#,
        )
        .unwrap();
        conn
    }

    fn insert_entry(conn: &Connection, id: &str, text: &str, created: &str) {
        conn.execute(
            "INSERT INTO entries (id, text, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)",
            rusqlite::params![id, text, created],
        )
        .unwrap();
    }

    fn body_of(conn: &Connection, id: &str) -> String {
        conn.query_row("SELECT body FROM fragments WHERE id = ?1", [id], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn fresh_database_reaches_target_version() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn, None).unwrap();
        assert_eq!(current_version(&conn).unwrap(), TARGET_VERSION);
    }

    #[test]
    fn every_entry_becomes_a_fragment_with_text_and_time_intact() {
        let mut conn = v1_db();
        insert_entry(&conn, "e1", "pasta water too salty again. less.", "2024-03-01T10:00:00Z");
        insert_entry(&conn, "e2", "  leading and trailing space kept  ", "2024-03-02T10:00:00Z");
        // A fragment can be a single word, a URL, or an empty-looking scrap. All equal intake.
        insert_entry(&conn, "e3", "ok", "2024-03-03T10:00:00Z");

        run(&mut conn, None).unwrap();

        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM fragments", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 3);
        assert_eq!(body_of(&conn, "e1"), "pasta water too salty again. less.");
        assert_eq!(body_of(&conn, "e2"), "  leading and trailing space kept  ");
        assert_eq!(body_of(&conn, "e3"), "ok");

        let captured: String = conn
            .query_row("SELECT captured_at FROM fragments WHERE id = 'e1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(captured, "2024-03-01T10:00:00Z");
    }

    #[test]
    fn v1_entries_survive_the_migration_untouched() {
        let mut conn = v1_db();
        insert_entry(&conn, "e1", "the original", "2024-03-01T10:00:00Z");
        run(&mut conn, None).unwrap();

        // Rollback safety: reverting the binary must still find the old world intact.
        let text: String = conn
            .query_row("SELECT text FROM entries WHERE id = 'e1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(text, "the original");
    }

    #[test]
    fn migration_is_idempotent() {
        let mut conn = v1_db();
        insert_entry(&conn, "e1", "once", "2024-03-01T10:00:00Z");

        run(&mut conn, None).unwrap();
        run(&mut conn, None).unwrap();
        run(&mut conn, None).unwrap();

        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM fragments", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "re-running the migration must not duplicate material");
    }

    /// The load-bearing one. v1's `continuation_from` is a character offset inside a single
    /// entry's own text. v2's Continue is a relationship between two separately dated
    /// fragments. Turning the former into the latter would fabricate a continuation the
    /// person never made, and would date it wrongly.
    #[test]
    fn legacy_in_text_continuation_is_preserved_but_creates_no_line() {
        let mut conn = v1_db();
        conn.execute(
            "INSERT INTO entries (id, text, created_at, updated_at, continuation_from, continuation_at) \
             VALUES ('e1', 'Original.\nContinued later.', '2024-03-01T10:00:00Z', '2024-03-05T08:00:00Z', 9, '2024-03-05T08:00:00Z')",
            [],
        )
        .unwrap();

        run(&mut conn, None).unwrap();

        // Text stays whole — the fragment is not split at the offset.
        assert_eq!(body_of(&conn, "e1"), "Original.\nContinued later.");

        let (offset, at): (Option<i64>, Option<String>) = conn
            .query_row(
                "SELECT legacy_continuation_offset, legacy_continuation_at FROM fragments WHERE id = 'e1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(offset, Some(9), "the marker is kept as provenance");
        assert_eq!(at.as_deref(), Some("2024-03-05T08:00:00Z"));

        let edges: i64 = conn
            .query_row("SELECT COUNT(*) FROM continuations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(edges, 0, "an in-text offset must never become a Line edge");

        let lines: i64 = conn
            .query_row("SELECT COUNT(*) FROM line_index", [], |r| r.get(0))
            .unwrap();
        assert_eq!(lines, 0);
    }

    /// v1 overwrote text in place, so prior wordings are already lost. The migration must
    /// not claim a correction history it does not hold.
    #[test]
    fn destructive_v1_edits_do_not_fabricate_revision_history() {
        let mut conn = v1_db();
        conn.execute(
            "INSERT INTO entries (id, text, created_at, updated_at, edit_count) \
             VALUES ('e1', 'the wording as it stands today', '2024-03-01T10:00:00Z', '2024-06-01T10:00:00Z', 3)",
            [],
        )
        .unwrap();

        run(&mut conn, None).unwrap();

        let (corrections, legacy_edits): (i64, i64) = conn
            .query_row(
                "SELECT correction_count, legacy_edit_count FROM fragments WHERE id = 'e1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();

        let revisions: i64 = conn
            .query_row("SELECT COUNT(*) FROM fragment_revisions", [], |r| r.get(0))
            .unwrap();

        assert_eq!(revisions, 0, "no prior wording exists, so none may be invented");
        assert_eq!(
            corrections, revisions,
            "correction_count must always equal the number of revisions actually held"
        );
        assert_eq!(legacy_edits, 3, "what v1 did know is recorded, just not as history");
    }

    #[test]
    fn capture_method_is_not_guessed_for_legacy_material() {
        let mut conn = v1_db();
        insert_entry(&conn, "e1", "could have been typed, spoken or shared", "2024-03-01T10:00:00Z");
        run(&mut conn, None).unwrap();

        let (method, origin): (String, Option<String>) = conn
            .query_row(
                "SELECT capture_method, capture_origin FROM fragments WHERE id = 'e1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(method, "imported", "v1 never recorded how a thing was captured");
        assert_eq!(origin.as_deref(), Some("legacy"));
    }

    #[test]
    fn pins_become_holds_and_keep_their_time() {
        let mut conn = v1_db();
        insert_entry(&conn, "e1", "boiler guy — Tues between 12 and 3", "2024-03-01T10:00:00Z");
        insert_entry(&conn, "e2", "not pinned", "2024-03-02T10:00:00Z");
        conn.execute(
            "INSERT INTO pinned_entries (entry_id, pinned_at) VALUES ('e1', '2024-03-04T09:00:00Z')",
            [],
        )
        .unwrap();

        run(&mut conn, None).unwrap();

        let (held_at, released): (String, Option<String>) = conn
            .query_row(
                "SELECT held_at, released_at FROM holds WHERE fragment_id = 'e1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(held_at, "2024-03-04T09:00:00Z");
        assert!(released.is_none(), "a migrated pin is still being kept present");

        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM holds", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn a_pin_pointing_at_a_missing_entry_does_not_break_the_migration() {
        let mut conn = v1_db();
        conn.execute(
            "INSERT INTO pinned_entries (entry_id, pinned_at) VALUES ('ghost', '2024-03-04T09:00:00Z')",
            [],
        )
        .unwrap();
        run(&mut conn, None).unwrap();

        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM holds", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn themes_are_written_out_before_the_concept_leaves() {
        let dir = std::env::temp_dir().join(format!("chinotto-migrate-{}", uuid::Uuid::new_v4()));
        let mut conn = v1_db();
        insert_entry(&conn, "e1", "something", "2024-03-01T10:00:00Z");
        conn.execute(
            "INSERT INTO user_themes (id, label, keywords, sort_order, created_at) \
             VALUES ('book', 'Book', '[]', 1, '2024-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO entry_themes (entry_id, theme_id, confidence, source, locked, classified_at) \
             VALUES ('e1', 'book', 0.8, 'auto', 0, '2024-02-01T00:00:00Z')",
            [],
        )
        .unwrap();

        run(&mut conn, Some(&dir)).unwrap();

        let text = std::fs::read_to_string(dir.join("themes-archive.json"))
            .expect("theme archive should be written next to the database");
        assert!(text.contains("\"Book\""), "the label a person typed must survive");
        assert!(text.contains("\"book\""));
        assert!(text.contains("\"e1\""));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn line_index_orders_a_chain_of_moments() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn, None).unwrap();

        for (id, at) in [
            ("a", "2021-03-29T10:00:00Z"),
            ("b", "2023-03-14T23:41:00Z"),
            ("c", "2024-11-02T08:19:00Z"),
        ] {
            conn.execute(
                "INSERT INTO fragments (id, body, captured_at) VALUES (?1, ?1, ?2)",
                rusqlite::params![id, at],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO continuations (fragment_id, continues_id, linked_at, origin) \
             VALUES ('b', 'a', '2023-03-14T23:41:00Z', 'explicit'), \
                    ('c', 'b', '2024-11-02T08:19:00Z', 'explicit')",
            [],
        )
        .unwrap();

        let tx = conn.transaction().unwrap();
        rebuild_line_index(&tx).unwrap();
        tx.commit().unwrap();

        let mut stmt = conn
            .prepare("SELECT fragment_id, root_id, position, line_length FROM line_index ORDER BY position")
            .unwrap();
        let rows: Vec<(String, String, i64, i64)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();

        assert_eq!(
            rows,
            vec![
                ("a".into(), "a".into(), 0, 3),
                ("b".into(), "a".into(), 1, 3),
                ("c".into(), "a".into(), 2, 3),
            ],
            "a line reads from its first moment forward, rooted at the original"
        );
    }

    #[test]
    fn rebuilding_the_line_index_is_safe_to_repeat() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn, None).unwrap();
        conn.execute_batch(
            "INSERT INTO fragments (id, body, captured_at) VALUES ('a','a','2021-01-01T00:00:00Z'),('b','b','2022-01-01T00:00:00Z');
             INSERT INTO continuations (fragment_id, continues_id, linked_at, origin) VALUES ('b','a','2022-01-01T00:00:00Z','explicit');",
        ).unwrap();

        for _ in 0..3 {
            let tx = conn.transaction().unwrap();
            rebuild_line_index(&tx).unwrap();
            tx.commit().unwrap();
        }

        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM line_index", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 2);
    }

    #[test]
    fn v3_adds_the_meaning_cache_and_reuses_v1_vectors() {
        let mut conn = v1_db();
        conn.execute_batch(
            "CREATE TABLE entry_embeddings (entry_id TEXT PRIMARY KEY, embedding BLOB NOT NULL);",
        )
        .unwrap();
        insert_entry(&conn, "e1", "something", "2024-03-01T10:00:00Z");
        conn.execute(
            "INSERT INTO entry_embeddings (entry_id, embedding) VALUES ('e1', X'00112233')",
            [],
        )
        .unwrap();

        run(&mut conn, None).unwrap();

        assert_eq!(current_version(&conn).unwrap(), TARGET_VERSION);
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM fragment_embeddings", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "work v1 already did is carried over, not recomputed");

        // v1 vectors arrive without provenance, which is how they get recomputed lazily.
        let (model, hash): (String, String) = conn
            .query_row(
                "SELECT model, body_hash FROM fragment_embeddings WHERE fragment_id = 'e1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(model, "AllMiniLML6V2");
        assert_eq!(hash, "", "no known source wording, so it is treated as stale");
    }

    #[test]
    fn an_older_database_upgrades_to_current_without_touching_material() {
        let mut conn = v1_db();
        insert_entry(&conn, "e1", "the original wording", "2024-03-01T10:00:00Z");

        // Stop at v2, the way an older binary would have left it.
        {
            let tx = conn.transaction().unwrap();
            migrate_to_v2(&tx, None).unwrap();
            set_version(&tx, 2).unwrap();
            tx.commit().unwrap();
        }
        assert_eq!(current_version(&conn).unwrap(), 2);

        run(&mut conn, None).unwrap();

        assert_eq!(current_version(&conn).unwrap(), TARGET_VERSION);
        let body: String = conn
            .query_row("SELECT body FROM fragments WHERE id = 'e1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(body, "the original wording");
    }

    #[test]
    fn a_fragment_cannot_continue_two_earlier_moments() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn, None).unwrap();
        conn.execute_batch(
            "INSERT INTO fragments (id, body, captured_at) VALUES ('a','a','2021-01-01T00:00:00Z'),('b','b','2021-02-01T00:00:00Z'),('c','c','2022-01-01T00:00:00Z');
             INSERT INTO continuations (fragment_id, continues_id, linked_at, origin) VALUES ('c','a','2022-01-01T00:00:00Z','explicit');",
        ).unwrap();

        let second = conn.execute(
            "INSERT INTO continuations (fragment_id, continues_id, linked_at, origin) VALUES ('c','b','2022-01-02T00:00:00Z','explicit')",
            [],
        );
        assert!(second.is_err(), "a line is a chain, so a moment continues at most one other");
    }
}

/// Regression: a v1 database shaped like one somebody actually used, and a corpus large
/// enough that a linear mistake would show.
#[cfg(test)]
mod regression_tests {
    use super::*;
    use rusqlite::Connection;

    /// Messy on purpose: edited entries, in-text continuations, pins pointing at edited
    /// rows, themes both user-made and machine-assigned, entries with only whitespace,
    /// duplicate text, unicode, and one entry with an empty body.
    fn realistic_v1(n: usize) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE entries (
              id TEXT PRIMARY KEY, text TEXT NOT NULL, created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL DEFAULT '', edit_count INTEGER DEFAULT 0,
              open_count INTEGER DEFAULT 0, space_id TEXT,
              continuation_from INTEGER, continuation_at TEXT
            );
            CREATE TABLE spaces (id TEXT PRIMARY KEY, label TEXT NOT NULL, sort_order INTEGER NOT NULL);
            CREATE TABLE pinned_entries (entry_id TEXT PRIMARY KEY, pinned_at TEXT NOT NULL);
            CREATE TABLE user_themes (id TEXT PRIMARY KEY, label TEXT NOT NULL, keywords TEXT NOT NULL,
              sort_order INTEGER NOT NULL, created_at TEXT NOT NULL);
            CREATE TABLE entry_themes (entry_id TEXT PRIMARY KEY, theme_id TEXT NOT NULL,
              confidence REAL NOT NULL, source TEXT NOT NULL, locked INTEGER NOT NULL DEFAULT 0,
              classified_at TEXT NOT NULL);
            CREATE TABLE entry_embeddings (entry_id TEXT PRIMARY KEY, embedding BLOB NOT NULL);
            INSERT INTO spaces VALUES ('work','Work',1),('personal','Personal',2);
            INSERT INTO user_themes VALUES ('book','Book','[]',1,'2024-01-01T00:00:00Z');
            "#,
        )
        .unwrap();

        let bodies = [
            "dinner friday — ask Lena if 8 works",
            "ok",
            "",
            "   ",
            "読み返すと、やっぱり同じことを考えている",
            "https://example.com/a?utm_source=x",
            "the same text twice",
            "the same text twice",
            "a much longer thought that runs on for a while and contains \"quotes\" and — dashes",
        ];
        for i in 0..n {
            let body = bodies[i % bodies.len()];
            let day = (i % 28) + 1;
            let month = (i % 12) + 1;
            let year = 2021 + (i % 5);
            let created = format!("{year}-{month:02}-{day:02}T09:{:02}:00Z", i % 60);
            let edits = if i % 7 == 0 { 3 } else { 0 };
            let space = if i % 3 == 0 { Some("work") } else { None };
            conn.execute(
                "INSERT INTO entries (id, text, created_at, updated_at, edit_count, space_id, \
                 continuation_from, continuation_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![
                    format!("e{i}"),
                    body,
                    created,
                    created,
                    edits,
                    space,
                    if i % 11 == 0 && body.len() > 4 { Some(4i64) } else { None },
                    if i % 11 == 0 && body.len() > 4 { Some(created.clone()) } else { None },
                ],
            )
            .unwrap();
            if i % 13 == 0 {
                conn.execute(
                    "INSERT INTO entry_themes VALUES (?1,'book',0.8,'auto',0,'2024-02-01T00:00:00Z')",
                    [format!("e{i}")],
                )
                .unwrap();
            }
            if i % 5 == 0 {
                conn.execute(
                    "INSERT INTO entry_embeddings VALUES (?1, X'0000803F')",
                    [format!("e{i}")],
                )
                .unwrap();
            }
        }
        // Five pins, the v1 maximum, some on edited entries.
        for i in [0usize, 7, 14, 21, 28] {
            conn.execute(
                "INSERT INTO pinned_entries VALUES (?1, '2025-06-01T00:00:00Z')",
                [format!("e{i}")],
            )
            .unwrap();
        }
        conn
    }

    #[test]
    fn a_realistic_v1_database_migrates_losing_nothing() {
        let mut conn = realistic_v1(400);

        let before: Vec<(String, String, String)> = {
            let mut stmt = conn
                .prepare("SELECT id, text, created_at FROM entries ORDER BY id")
                .unwrap();
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .unwrap()
                .map(|r| r.unwrap())
                .collect()
        };

        run(&mut conn, None).unwrap();

        // Every entry, byte-for-byte, with its original time.
        let after: Vec<(String, String, String)> = {
            let mut stmt = conn
                .prepare("SELECT id, body, captured_at FROM fragments ORDER BY id")
                .unwrap();
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .unwrap()
                .map(|r| r.unwrap())
                .collect()
        };
        assert_eq!(after, before, "no entry may change text or date in migration");

        // Nothing was promoted into a Line.
        let edges: i64 = conn
            .query_row("SELECT COUNT(*) FROM continuations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(edges, 0);

        // No fabricated correction history anywhere.
        let revisions: i64 = conn
            .query_row("SELECT COUNT(*) FROM fragment_revisions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(revisions, 0);
        let bad: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM fragments WHERE correction_count <> 0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(bad, 0);
        // …but what v1 did know is kept.
        let legacy_edits: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM fragments WHERE legacy_edit_count > 0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(legacy_edits > 0);

        // All five pins became holds.
        let holds: i64 = conn
            .query_row("SELECT COUNT(*) FROM holds WHERE released_at IS NULL", [], |r| r.get(0))
            .unwrap();
        assert_eq!(holds, 5);

        // v1 embeddings carried over rather than being recomputed.
        let vectors: i64 = conn
            .query_row("SELECT COUNT(*) FROM fragment_embeddings", [], |r| r.get(0))
            .unwrap();
        assert_eq!(vectors, 80, "400 / 5");

        // v1 is still intact underneath.
        let entries_still: i64 = conn
            .query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0))
            .unwrap();
        assert_eq!(entries_still, 400);
    }

    #[test]
    fn migrating_a_realistic_database_twice_changes_nothing() {
        let mut conn = realistic_v1(200);
        run(&mut conn, None).unwrap();
        let first: i64 = conn
            .query_row("SELECT COUNT(*) FROM fragments", [], |r| r.get(0))
            .unwrap();
        run(&mut conn, None).unwrap();
        run(&mut conn, None).unwrap();
        let again: i64 = conn
            .query_row("SELECT COUNT(*) FROM fragments", [], |r| r.get(0))
            .unwrap();
        assert_eq!(first, again);
        assert_eq!(current_version(&conn).unwrap(), TARGET_VERSION);
    }

    /// Reading the Record must not get slower in proportion to how much of it there is.
    #[test]
    fn a_large_corpus_stays_responsive() {
        use crate::db::Db;
        use std::time::Instant;

        let db = Db::open(std::path::PathBuf::from(":memory:")).unwrap();
        {
            let conn = db.0.lock().unwrap();
            conn.execute_batch("BEGIN").unwrap();
            for i in 0..20_000 {
                let year = 2019 + (i % 7);
                let month = (i % 12) + 1;
                let day = (i % 28) + 1;
                conn.execute(
                    "INSERT INTO fragments (id, body, captured_at, capture_method) \
                     VALUES (?1, ?2, ?3, 'typed')",
                    rusqlite::params![
                        format!("f{i}"),
                        format!("fragment number {i} about folders and premature judgment"),
                        format!("{year}-{month:02}-{day:02}T10:00:00Z")
                    ],
                )
                .unwrap();
            }
            conn.execute_batch("COMMIT").unwrap();
            // Index it the way the app does.
            conn.execute_batch(
                "INSERT INTO fragments_fts (body, transcript, source, fragment_id) \
                 SELECT body, '', '', id FROM fragments",
            )
            .unwrap();
        }

        let t = Instant::now();
        let now_page = db.recent_fragments(120).unwrap();
        let now_ms = t.elapsed().as_millis();
        assert_eq!(now_page.len(), 120);

        let t = Instant::now();
        let deep = db
            .fragments_before(Some("2020-01-01T00:00:00Z"), 100)
            .unwrap();
        let deep_ms = t.elapsed().as_millis();
        assert!(!deep.is_empty());

        let t = Instant::now();
        let found = db.find_fragments("premature", 200).unwrap();
        let find_ms = t.elapsed().as_millis();
        assert_eq!(found.len(), 200);

        let t = Instant::now();
        let density = db.month_density(2021).unwrap();
        let density_ms = t.elapsed().as_millis();
        assert_eq!(density.len(), 12);

        // Generous bounds: this is a guard against an accidental full scan, not a benchmark.
        assert!(now_ms < 150, "Now took {now_ms}ms at 20k fragments");
        assert!(deep_ms < 150, "deep page took {deep_ms}ms");
        assert!(find_ms < 400, "find took {find_ms}ms");
        assert!(density_ms < 250, "density took {density_ms}ms");
    }

    /// A database built by an earlier build of THIS branch is already at v4, so `run()`
    /// used to return early and `return_evidence.related_id` never arrived. Every
    /// `select_return` then failed on "no such column" and Returns vanished silently.
    #[test]
    fn a_v4_record_still_gains_the_returns_cause_column() {
        let path = std::env::temp_dir().join(format!(
            "chinotto-v4-upgrade-{}.sqlite3",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);

        {
            // Build the Record as the earlier build left it: v4, and no `related_id`.
            let mut conn = Connection::open(&path).unwrap();
            super::run(&mut conn, None).unwrap();
            conn.execute_batch(
                "DROP TABLE return_evidence;
                 CREATE TABLE return_evidence (
                   id           INTEGER PRIMARY KEY AUTOINCREMENT,
                   return_id    INTEGER NOT NULL REFERENCES returns(id) ON DELETE CASCADE,
                   kind         TEXT NOT NULL,
                   detail       TEXT NOT NULL,
                   occurred_at  TEXT
                 );
                 PRAGMA user_version = 4;",
            )
            .unwrap();
            assert!(!column_exists_conn(&conn, "return_evidence", "related_id"));
        }

        // Opening the way the app does runs the ladder.
        let db = super::super::Db::open(path.clone()).unwrap();
        {
            let conn = db.0.lock().unwrap();
            assert_eq!(current_version(&conn).unwrap(), TARGET_VERSION);
            assert!(
                column_exists_conn(&conn, "return_evidence", "related_id"),
                "a Return could not say what caused it, so it could not be shown at all"
            );
        }

        // And the read path works, rather than merely the column existing.
        assert!(db
            .select_return("2026-09-19T17:10:00+00:00")
            .unwrap()
            .is_none());

        drop(db);
        let _ = std::fs::remove_file(&path);
    }

    fn column_exists_conn(conn: &Connection, table: &str, column: &str) -> bool {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})")).unwrap();
        let mut rows = stmt.query([]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            let name: String = row.get(1).unwrap();
            if name == column {
                return true;
            }
        }
        false
    }
}
