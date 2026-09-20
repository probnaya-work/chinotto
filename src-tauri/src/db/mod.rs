pub mod bridge;
pub mod material;
pub mod meaning;
pub mod migrate;
pub mod record;
pub mod returns;
mod schema;

use chrono::{DateTime, Datelike, Local, NaiveDate, Utc};
use rusqlite::Connection;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Mutex;

fn utc_dt_from_created_at(iso: &str) -> Option<DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(iso)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn local_naive_date_from_created_at(iso: &str) -> Option<NaiveDate> {
    let utc = utc_dt_from_created_at(iso)?;
    Some(utc.with_timezone(&Local).date_naive())
}

/// Build an FTS5 prefix query so partial words match (e.g. "cap" matches "capture").
/// Tokens are space-separated; each becomes "token*". Special chars " and - are escaped.
fn fts5_prefix_query(user_query: &str) -> String {
    let q = user_query.trim();
    if q.is_empty() {
        return String::new();
    }
    let tokens: Vec<String> = q
        .split_whitespace()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|token| {
            let escaped = token.replace('"', "\"\"");
            if token.contains('"') || token.contains('-') {
                format!("\"{}\"*", escaped)
            } else {
                format!("{}*", token)
            }
        })
        .collect();
    tokens.join(" ")
}

/// Escape % and _ for use in SQLite LIKE (so they match literally).
fn escape_like(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c == '%' || c == '_' {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Wrap each case-insensitive occurrence of `query` in `text` with FTS highlight markers.
fn highlight_substring(text: &str, query: &str) -> String {
    let q = query.trim();
    if q.is_empty() {
        return text.to_string();
    }
    let text_lower = text.to_lowercase();
    let q_lower = q.to_lowercase();
    let mut out = String::with_capacity(text.len() + 32);
    let mut start = 0;
    while let Some(pos) = text_lower[start..].find(&q_lower) {
        let abs = start + pos;
        out.push_str(&text[start..abs]);
        out.push('\u{0001}');
        out.push_str(&text[abs..abs + q.len()]);
        out.push('\u{0002}');
        start = abs + q.len();
    }
    out.push_str(&text[start..]);
    out
}

/// Stream lens: all entries, Inbox-only (`space_id` NULL), or a row from `spaces`.
#[derive(Clone, Debug, Default)]
pub enum SpaceFilter {
    #[default]
    All,
    Inbox,
    Space(String),
}

#[derive(Clone, Debug)]
pub struct SpaceRow {
    pub id: String,
    pub label: String,
    pub sort_order: i32,
}

#[derive(Clone, Debug)]
pub struct EntryThemeRow {
    pub theme_id: String,
    pub confidence: f64,
    pub source: String,
    pub locked: bool,
}

#[derive(Clone, Debug)]
pub struct UserThemeRow {
    pub id: String,
    pub label: String,
    pub sort_order: i32,
}

/// Max user-defined recall themes (system "links" is separate).
pub const MAX_USER_THEMES: usize = 7;

const ENTRY_SELECT: &str =
    "id, text, created_at, updated_at, COALESCE(edit_count, 0), COALESCE(open_count, 0), space_id, continuation_from, continuation_at";

fn ensure_spaces(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS spaces (
            id TEXT PRIMARY KEY,
            label TEXT NOT NULL,
            sort_order INTEGER NOT NULL
        );
        INSERT OR IGNORE INTO spaces (id, label, sort_order) VALUES ('work', 'Work', 1);
        INSERT OR IGNORE INTO spaces (id, label, sort_order) VALUES ('personal', 'Personal', 2);
    "#,
    )?;
    let has_column = conn.query_row(
        "SELECT 1 FROM pragma_table_info('entries') WHERE name = 'space_id'",
        [],
        |r| r.get::<_, i32>(0),
    );
    if !matches!(has_column, Ok(1)) {
        conn.execute(
            "ALTER TABLE entries ADD COLUMN space_id TEXT REFERENCES spaces(id) ON DELETE SET NULL",
            [],
        )?;
    }
    Ok(())
}

/// Minimum confidence for theme recall filters (search chips, theme browse).
pub const THEME_RECALL_MIN_CONFIDENCE: f64 = 0.7;

pub struct Db(Mutex<Connection>);

fn ensure_importance_columns(conn: &Connection) -> Result<(), rusqlite::Error> {
    let has_column = conn.query_row(
        "SELECT 1 FROM pragma_table_info('entries') WHERE name = 'edit_count'",
        [],
        |r| r.get::<_, i32>(0),
    );
    match has_column {
        Ok(1) => return Ok(()),
        _ => {}
    }
    conn.execute(
        "ALTER TABLE entries ADD COLUMN edit_count INTEGER DEFAULT 0",
        [],
    )?;
    conn.execute(
        "ALTER TABLE entries ADD COLUMN open_count INTEGER DEFAULT 0",
        [],
    )?;
    Ok(())
}

fn ensure_continuation_columns(conn: &Connection) -> Result<(), rusqlite::Error> {
    for col in ["continuation_from", "continuation_at"] {
        let has_column = conn.query_row(
            &format!("SELECT 1 FROM pragma_table_info('entries') WHERE name = '{col}'"),
            [],
            |r| r.get::<_, i32>(0),
        );
        if !matches!(has_column, Ok(1)) {
            let sql = if col == "continuation_from" {
                "ALTER TABLE entries ADD COLUMN continuation_from INTEGER"
            } else {
                "ALTER TABLE entries ADD COLUMN continuation_at TEXT"
            };
            conn.execute(sql, [])?;
        }
    }
    Ok(())
}

/// JS string index (UTF-16 code units) → byte index in Rust `str`.
fn js_offset_to_byte(text: &str, js_offset: usize) -> Option<usize> {
    let mut utf16 = 0usize;
    for (byte_i, ch) in text.char_indices() {
        if utf16 == js_offset {
            return Some(byte_i);
        }
        utf16 += ch.len_utf16();
    }
    if utf16 == js_offset {
        return Some(text.len());
    }
    None
}

fn continuation_marker_valid(text: &str, js_offset: i32) -> bool {
    if js_offset < 1 {
        return false;
    }
    let Some(byte_idx) = js_offset_to_byte(text, js_offset as usize) else {
        return false;
    };
    byte_idx > 0 && text.as_bytes().get(byte_idx - 1) == Some(&b'\n')
}

fn ensure_share_threads_table(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS share_threads (
            token TEXT PRIMARY KEY,
            entry_ids TEXT NOT NULL,
            context_note TEXT,
            created_at TEXT NOT NULL,
            expires_at TEXT NOT NULL,
            revoked_at TEXT
        );
        "#,
    )?;
    Ok(())
}

fn ensure_user_themes_table(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS user_themes (
            id TEXT PRIMARY KEY,
            label TEXT NOT NULL,
            keywords TEXT NOT NULL,
            sort_order INTEGER NOT NULL,
            created_at TEXT NOT NULL
        );
        "#,
    )?;
    let count: i32 = conn.query_row("SELECT COUNT(*) FROM user_themes", [], |r| r.get(0))?;
    if count > 0 {
        return Ok(());
    }
    let created_at = chrono::Utc::now().to_rfc3339();
    let seeds: [(&str, &str, i32); 2] = [("book", "Book", 1), ("therapy", "Therapy", 2)];
    for (id, label, sort_order) in seeds {
        conn.execute(
            "INSERT INTO user_themes (id, label, keywords, sort_order, created_at) VALUES (?1, ?2, '[]', ?3, ?4)",
            rusqlite::params![id, label, sort_order, created_at],
        )?;
    }
    Ok(())
}

fn ensure_updated_at_column(conn: &Connection) -> Result<(), rusqlite::Error> {
    let has_column = conn.query_row(
        "SELECT 1 FROM pragma_table_info('entries') WHERE name = 'updated_at'",
        [],
        |r| r.get::<_, i32>(0),
    );
    if !matches!(has_column, Ok(1)) {
        conn.execute(
            "ALTER TABLE entries ADD COLUMN updated_at TEXT NOT NULL DEFAULT ''",
            [],
        )?;
        conn.execute(
            "UPDATE entries SET updated_at = created_at WHERE updated_at = ''",
            [],
        )?;
    }
    Ok(())
}

impl Db {
    pub fn open(path: PathBuf) -> Result<Self, rusqlite::Error> {
        // ":memory:" and bare filenames have no meaningful parent; writing the theme
        // archive relative to the current working directory would litter wherever the app
        // happened to be launched from.
        let archive_dir = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| p.to_path_buf());
        let mut conn = Connection::open(path)?;
        conn.pragma_update(None, "foreign_keys", true)?;
        schema::run_migrations(&conn)?;
        ensure_importance_columns(&conn)?;
        ensure_updated_at_column(&conn)?;
        ensure_continuation_columns(&conn)?;
        ensure_share_threads_table(&conn)?;
        ensure_user_themes_table(&conn)?;
        ensure_spaces(&conn)?;
        // v1 ensure_* calls above must run first: the v2 migration reads the columns they add.
        migrate::run(&mut conn, archive_dir.as_deref())?;
        Ok(Self(Mutex::new(conn)))
    }

    /// Returns true if `id` exists in `spaces` (for assignable space ids, not Inbox).
    pub fn space_id_valid(&self, id: &str) -> Result<bool, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let n: i32 = conn.query_row(
            "SELECT COUNT(*) FROM spaces WHERE id = ?1",
            [id],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    pub fn list_spaces(&self) -> Result<Vec<SpaceRow>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, label, sort_order FROM spaces ORDER BY sort_order ASC, id ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(SpaceRow {
                id: r.get(0)?,
                label: r.get(1)?,
                sort_order: r.get(2)?,
            })
        })?;
        rows.collect()
    }

    pub fn create_entry(
        &self,
        id: &str,
        text: &str,
        created_at: &str,
        space_id: Option<&str>,
    ) -> Result<(), rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        conn.execute(
            "INSERT INTO entries (id, text, created_at, updated_at, space_id) VALUES (?1, ?2, ?3, ?3, ?4)",
            rusqlite::params![id, text, created_at, space_id],
        )?;
        conn.execute(
            "DELETE FROM firestore_ingest_suppressed_ids WHERE id = ?1",
            [id],
        )?;
        Ok(())
    }

    /// Firestore ingest (mobile sync.md): insert only when `id` is new; idempotent on retries.
    /// Skips empty text or unparseable `created_at` (RFC3339).
    pub fn ingest_firestore_entries(
        &self,
        entries: &[(String, String, String)],
    ) -> Result<u32, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let mut inserted: u32 = 0;
        for (id, text, created_at) in entries {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                continue;
            }
            if chrono::DateTime::parse_from_rfc3339(created_at).is_err() {
                continue;
            }
            let suppressed: i32 = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM firestore_ingest_suppressed_ids WHERE id = ?1)",
                [id.as_str()],
                |r| r.get(0),
            )?;
            if suppressed != 0 {
                continue;
            }
            let n = conn.execute(
                "INSERT OR IGNORE INTO entries (id, text, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)",
                [id.as_str(), trimmed, created_at.as_str()],
            )?;
            if n > 0 {
                inserted += 1;
            }
        }
        Ok(inserted)
    }

    pub fn update_entry_text(&self, id: &str, text: &str) -> Result<(), rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let updated_at = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE entries SET text = ?1, updated_at = ?2, edit_count = COALESCE(edit_count, 0) + 1 WHERE id = ?3",
            rusqlite::params![text, updated_at, id],
        )?;
        let cont_from: Option<i32> = conn.query_row(
            "SELECT continuation_from FROM entries WHERE id = ?1",
            [id],
            |r| r.get(0),
        )?;
        if let Some(from) = cont_from {
            if !continuation_marker_valid(text, from) {
                conn.execute(
                    "UPDATE entries SET continuation_from = NULL, continuation_at = NULL WHERE id = ?1",
                    [id],
                )?;
            }
        }
        Ok(())
    }

    /// Marks where a later continuation starts (UTF-16 code unit offset). Set once per entry.
    pub fn mark_entry_continuation(
        &self,
        id: &str,
        from_offset: i32,
        text: &str,
    ) -> Result<Option<(i32, String)>, rusqlite::Error> {
        if !continuation_marker_valid(text, from_offset) {
            return Ok(None);
        }
        let conn = self.0.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        let n = conn.execute(
            "UPDATE entries SET continuation_from = ?1, continuation_at = ?2 WHERE id = ?3 AND continuation_from IS NULL",
            rusqlite::params![from_offset, now, id],
        )?;
        if n == 0 {
            let existing: Option<(i32, String)> = conn
                .query_row(
                    "SELECT continuation_from, continuation_at FROM entries WHERE id = ?1 AND continuation_from IS NOT NULL",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .ok();
            return Ok(existing);
        }
        Ok(Some((from_offset, now)))
    }

    pub fn update_entry_space(
        &self,
        id: &str,
        space_id: Option<&str>,
    ) -> Result<(), rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let updated_at = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE entries SET space_id = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![space_id, updated_at, id],
        )?;
        Ok(())
    }

    pub fn record_entry_open(&self, entry_id: &str) -> Result<(), rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        conn.execute(
            "UPDATE entries SET open_count = COALESCE(open_count, 0) + 1 WHERE id = ?1",
            [entry_id],
        )?;
        Ok(())
    }

    pub fn list_entries(&self) -> Result<Vec<EntryRow>, rusqlite::Error> {
        self.list_entries_filtered(&SpaceFilter::All)
    }

    pub fn list_entries_filtered(
        &self,
        filter: &SpaceFilter,
    ) -> Result<Vec<EntryRow>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        match filter {
            SpaceFilter::All => {
                let sql = format!(
                    "SELECT {ENTRY_SELECT} FROM entries ORDER BY created_at DESC, id ASC"
                );
                let mut stmt = conn.prepare(&sql)?;
                let out: Vec<EntryRow> = stmt.query_map([], row_to_entry)?.collect::<Result<_, _>>()?;
                Ok(out)
            }
            SpaceFilter::Inbox => {
                let sql = format!(
                    "SELECT {ENTRY_SELECT} FROM entries WHERE space_id IS NULL ORDER BY created_at DESC, id ASC"
                );
                let mut stmt = conn.prepare(&sql)?;
                let out: Vec<EntryRow> = stmt.query_map([], row_to_entry)?.collect::<Result<_, _>>()?;
                Ok(out)
            }
            SpaceFilter::Space(id) => {
                let sql = format!(
                    "SELECT {ENTRY_SELECT} FROM entries WHERE space_id = ?1 ORDER BY created_at DESC, id ASC"
                );
                let mut stmt = conn.prepare(&sql)?;
                let out: Vec<EntryRow> =
                    stmt.query_map([id.as_str()], row_to_entry)?.collect::<Result<_, _>>()?;
                Ok(out)
            }
        }
    }

    /// Local calendar dates (YYYY-MM-DD) in `[year, month]` that have at least one entry.
    pub fn local_entry_dates_in_month(
        &self,
        year: i32,
        month: u32,
        filter: &SpaceFilter,
    ) -> Result<Vec<String>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let mut set = BTreeSet::new();
        let created_ats: Vec<String> = match filter {
            SpaceFilter::All => {
                let mut stmt = conn.prepare("SELECT created_at FROM entries")?;
                let v: Vec<String> = stmt
                    .query_map([], |r| r.get::<_, String>(0))?
                    .collect::<Result<_, _>>()?;
                v
            }
            SpaceFilter::Inbox => {
                let mut stmt =
                    conn.prepare("SELECT created_at FROM entries WHERE space_id IS NULL")?;
                let v: Vec<String> = stmt
                    .query_map([], |r| r.get::<_, String>(0))?
                    .collect::<Result<_, _>>()?;
                v
            }
            SpaceFilter::Space(id) => {
                let mut stmt =
                    conn.prepare("SELECT created_at FROM entries WHERE space_id = ?1")?;
                let v: Vec<String> = stmt
                    .query_map([id.as_str()], |r| r.get::<_, String>(0))?
                    .collect::<Result<_, _>>()?;
                v
            }
        };
        for created_at in created_ats {
            let Some(d) = local_naive_date_from_created_at(&created_at) else {
                continue;
            };
            if d.year() == year && d.month() == month {
                set.insert(d.format("%Y-%m-%d").to_string());
            }
        }
        Ok(set.into_iter().collect())
    }

    /// Newest entry on the given local calendar day (first row for that day in `created_at DESC` stream order).
    pub fn jump_anchor_entry_id_for_local_date(
        &self,
        target: NaiveDate,
        filter: &SpaceFilter,
    ) -> Result<Option<String>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let pairs: Vec<(String, String)> = match filter {
            SpaceFilter::All => {
                let mut stmt = conn.prepare("SELECT id, created_at FROM entries")?;
                let v: Vec<(String, String)> = stmt
                    .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                    .collect::<Result<_, _>>()?;
                v
            }
            SpaceFilter::Inbox => {
                let mut stmt =
                    conn.prepare("SELECT id, created_at FROM entries WHERE space_id IS NULL")?;
                let v: Vec<(String, String)> = stmt
                    .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                    .collect::<Result<_, _>>()?;
                v
            }
            SpaceFilter::Space(id) => {
                let mut stmt =
                    conn.prepare("SELECT id, created_at FROM entries WHERE space_id = ?1")?;
                let v: Vec<(String, String)> = stmt
                    .query_map([id.as_str()], |r| {
                        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                    })?
                    .collect::<Result<_, _>>()?;
                v
            }
        };
        let mut best: Option<(String, DateTime<Utc>)> = None;
        for (id, created_at) in pairs {
            let Some(d) = local_naive_date_from_created_at(&created_at) else {
                continue;
            };
            if d != target {
                continue;
            }
            let Some(ts) = utc_dt_from_created_at(&created_at) else {
                continue;
            };
            if best
                .as_ref()
                .map_or(true, |(_, t)| ts > *t)
            {
                best = Some((id, ts));
            }
        }
        Ok(best.map(|(id, _)| id))
    }

    pub fn get_entry_by_id(&self, id: &str) -> Result<Option<EntryRow>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let sql = format!("SELECT {ENTRY_SELECT} FROM entries WHERE id = ?1");
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query([id])?;
        if let Some(row) = rows.next()? {
            return Ok(Some(row_to_entry(&row)?));
        }
        Ok(None)
    }

    pub fn insert_embedding(
        &self,
        entry_id: &str,
        embedding: &[f32],
    ) -> Result<(), rusqlite::Error> {
        let blob: Vec<u8> = embedding.iter().flat_map(|f| f.to_le_bytes()).collect();
        let conn = self.0.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO entry_embeddings (entry_id, embedding) VALUES (?1, ?2)",
            rusqlite::params![entry_id, blob],
        )?;
        Ok(())
    }

    pub fn entry_theme_locked(&self, entry_id: &str) -> Result<bool, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let locked: Option<i32> = conn
            .query_row(
                "SELECT locked FROM entry_themes WHERE entry_id = ?1",
                [entry_id],
                |row| row.get(0),
            )
            .ok();
        Ok(locked.unwrap_or(0) != 0)
    }

    pub fn get_entry_theme(&self, entry_id: &str) -> Result<Option<EntryThemeRow>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT theme_id, confidence, source, locked FROM entry_themes WHERE entry_id = ?1",
        )?;
        let mut rows = stmt.query([entry_id])?;
        if let Some(row) = rows.next()? {
            return Ok(Some(EntryThemeRow {
                theme_id: row.get(0)?,
                confidence: row.get(1)?,
                source: row.get(2)?,
                locked: row.get::<_, i32>(3)? != 0,
            }));
        }
        Ok(None)
    }

    pub fn upsert_entry_theme(
        &self,
        entry_id: &str,
        theme_id: &str,
        confidence: f64,
        source: &str,
    ) -> Result<(), rusqlite::Error> {
        let classified_at = chrono::Utc::now().to_rfc3339();
        let conn = self.0.lock().unwrap();
        conn.execute(
            "INSERT INTO entry_themes (entry_id, theme_id, confidence, source, locked, classified_at)
             VALUES (?1, ?2, ?3, ?4, 0, ?5)
             ON CONFLICT(entry_id) DO UPDATE SET
               theme_id = excluded.theme_id,
               confidence = excluded.confidence,
               source = excluded.source,
               classified_at = excluded.classified_at
             WHERE locked = 0",
            rusqlite::params![entry_id, theme_id, confidence, source, classified_at],
        )?;
        Ok(())
    }

    pub fn clear_entry_theme_if_unlocked(&self, entry_id: &str) -> Result<(), rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        conn.execute(
            "DELETE FROM entry_themes WHERE entry_id = ?1 AND locked = 0",
            [entry_id],
        )?;
        Ok(())
    }

    pub fn list_user_themes(&self) -> Result<Vec<UserThemeRow>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, label, sort_order FROM user_themes ORDER BY sort_order, label",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(UserThemeRow {
                id: row.get(0)?,
                label: row.get(1)?,
                sort_order: row.get(2)?,
            })
        })?;
        rows.collect()
    }

    pub fn user_theme_count(&self) -> Result<usize, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let count: i32 = conn.query_row("SELECT COUNT(*) FROM user_themes", [], |r| r.get(0))?;
        Ok(count as usize)
    }

    pub fn theme_id_valid(&self, theme_id: &str) -> Result<bool, rusqlite::Error> {
        if theme_id == crate::themes::SYSTEM_THEME_LINKS {
            return Ok(true);
        }
        let conn = self.0.lock().unwrap();
        let n: i32 = conn.query_row(
            "SELECT COUNT(*) FROM user_themes WHERE id = ?1",
            [theme_id],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    pub fn create_user_theme(&self, label: &str) -> Result<UserThemeRow, String> {
        let label = label.trim();
        if label.is_empty() {
            return Err("theme label is required".into());
        }
        if self.user_theme_count().map_err(|e| e.to_string())? >= MAX_USER_THEMES {
            return Err(format!("maximum of {MAX_USER_THEMES} themes"));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let created_at = chrono::Utc::now().to_rfc3339();
        let sort_order: i32 = {
            let conn = self.0.lock().unwrap();
            let sort_order: i32 = conn
                .query_row(
                    "SELECT COALESCE(MAX(sort_order), 0) + 1 FROM user_themes",
                    [],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            conn.execute(
                "INSERT INTO user_themes (id, label, keywords, sort_order, created_at) VALUES (?1, ?2, '[]', ?3, ?4)",
                rusqlite::params![id, label, sort_order, created_at],
            )
            .map_err(|e| e.to_string())?;
            sort_order
        };
        self.enqueue_sync_user_theme_upsert(&id, label, sort_order)
            .map_err(|e| e.to_string())?;
        Ok(UserThemeRow {
            id,
            label: label.to_string(),
            sort_order,
        })
    }

    pub fn update_user_theme(&self, id: &str, label: &str) -> Result<UserThemeRow, String> {
        let label = label.trim();
        if label.is_empty() {
            return Err("theme label is required".into());
        }
        let existing = self
            .get_user_theme(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "theme not found".to_string())?;
        {
            let conn = self.0.lock().unwrap();
            conn.execute(
                "UPDATE user_themes SET label = ?2 WHERE id = ?1",
                rusqlite::params![id, label],
            )
            .map_err(|e| e.to_string())?;
        }
        self.enqueue_sync_user_theme_upsert(id, label, existing.sort_order)
            .map_err(|e| e.to_string())?;
        Ok(UserThemeRow {
            id: id.to_string(),
            label: label.to_string(),
            sort_order: existing.sort_order,
        })
    }

    pub fn delete_user_theme(&self, id: &str) -> Result<(), String> {
        if self.get_user_theme(id).map_err(|e| e.to_string())?.is_none() {
            return Err("theme not found".into());
        }
        self.enqueue_sync_user_theme_tombstone(id)
            .map_err(|e| e.to_string())?;
        self.add_user_theme_ingest_suppression(id)
            .map_err(|e| e.to_string())?;
        let conn = self.0.lock().unwrap();
        let changed = conn
            .execute("DELETE FROM user_themes WHERE id = ?1", [id])
            .map_err(|e| e.to_string())?;
        if changed == 0 {
            return Err("theme not found".into());
        }
        conn.execute("DELETE FROM entry_themes WHERE theme_id = ?1", [id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn get_user_theme(&self, id: &str) -> Result<Option<UserThemeRow>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, label, sort_order FROM user_themes WHERE id = ?1",
        )?;
        let mut rows = stmt.query([id])?;
        if let Some(row) = rows.next()? {
            return Ok(Some(UserThemeRow {
                id: row.get(0)?,
                label: row.get(1)?,
                sort_order: row.get(2)?,
            }));
        }
        Ok(None)
    }

    pub fn set_entry_theme(
        &self,
        entry_id: &str,
        theme_id: Option<&str>,
        locked: bool,
    ) -> Result<(), rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        match theme_id {
            Some(id) => {
                let classified_at = chrono::Utc::now().to_rfc3339();
                let locked_i = if locked { 1 } else { 0 };
                conn.execute(
                    "INSERT INTO entry_themes (entry_id, theme_id, confidence, source, locked, classified_at)
                     VALUES (?1, ?2, 1.0, 'manual', ?3, ?4)
                     ON CONFLICT(entry_id) DO UPDATE SET
                       theme_id = excluded.theme_id,
                       confidence = excluded.confidence,
                       source = excluded.source,
                       locked = excluded.locked,
                       classified_at = excluded.classified_at",
                    rusqlite::params![entry_id, id, locked_i, classified_at],
                )?;
            }
            None => {
                if locked {
                    conn.execute("DELETE FROM entry_themes WHERE entry_id = ?1", [entry_id])?;
                } else {
                    conn.execute(
                        "DELETE FROM entry_themes WHERE entry_id = ?1 AND locked = 0",
                        [entry_id],
                    )?;
                }
            }
        }
        Ok(())
    }

    pub fn get_embedding(&self, entry_id: &str) -> Result<Option<Vec<f32>>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT embedding FROM entry_embeddings WHERE entry_id = ?1")?;
        let mut rows = stmt.query([entry_id])?;
        if let Some(row) = rows.next()? {
            let blob: Vec<u8> = row.get(0)?;
            let embedding: Vec<f32> = blob
                .chunks_exact(4)
                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect();
            return Ok(Some(embedding));
        }
        Ok(None)
    }

    pub fn get_all_embeddings_excluding(
        &self,
        exclude_id: &str,
    ) -> Result<Vec<(String, Vec<f32>)>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT entry_id, embedding FROM entry_embeddings WHERE entry_id != ?1")?;
        let rows = stmt.query_map([exclude_id], |row| {
            let entry_id: String = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
            let embedding: Vec<f32> = blob
                .chunks_exact(4)
                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect();
            Ok((entry_id, embedding))
        })?;
        rows.collect()
    }

    pub fn get_entries_by_ids(&self, ids: &[String]) -> Result<Vec<EntryRow>, rusqlite::Error> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let conn = self.0.lock().unwrap();
        let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT {ENTRY_SELECT} FROM entries WHERE id IN ({})",
            placeholders
        );
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query(rusqlite::params_from_iter(ids.iter()))?;
        let mut out = vec![];
        while let Some(row) = rows.next()? {
            out.push(row_to_entry(&row)?);
        }
        Ok(out)
    }

    pub fn insert_pinned(&self, entry_id: &str) -> Result<(), rusqlite::Error> {
        let pinned_at = chrono::Utc::now().to_rfc3339();
        let conn = self.0.lock().unwrap();
        conn.execute(
            "INSERT INTO pinned_entries (entry_id, pinned_at) VALUES (?1, ?2)",
            [entry_id, &pinned_at],
        )?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM pinned_entries", [], |r| r.get(0))?;
        if count > 5 {
            conn.execute(
                "DELETE FROM pinned_entries WHERE entry_id = (
                  SELECT entry_id FROM pinned_entries ORDER BY pinned_at ASC LIMIT 1
                )",
                [],
            )?;
        }
        Ok(())
    }

    pub fn remove_pinned(&self, entry_id: &str) -> Result<(), rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        conn.execute("DELETE FROM pinned_entries WHERE entry_id = ?1", [entry_id])?;
        Ok(())
    }

    pub fn list_pinned_entry_ids(&self) -> Result<Vec<String>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT entry_id FROM pinned_entries ORDER BY pinned_at DESC")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect()
    }

    pub fn delete_entry(&self, entry_id: &str) -> Result<(), rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        conn.execute("DELETE FROM entries WHERE id = ?1", [entry_id])?;
        let at = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT OR REPLACE INTO firestore_ingest_suppressed_ids (id, suppressed_at) VALUES (?1, ?2)",
            [entry_id, at.as_str()],
        )?;
        Ok(())
    }

    /// Remove every entry and related rows (pins, embeddings). For local debug tooling.
    pub fn delete_all_entries(&self) -> Result<(), rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        conn.execute("DELETE FROM pinned_entries", [])?;
        conn.execute("DELETE FROM entry_embeddings", [])?;
        conn.execute("DELETE FROM entry_themes", [])?;
        conn.execute("DELETE FROM entries", [])?;
        conn.execute("DELETE FROM firestore_ingest_suppressed_ids", [])?;
        conn.execute("DELETE FROM sync_tombstone_outbox", [])?;
        conn.execute("DELETE FROM firestore_ingest_suppressed_theme_ids", [])?;
        conn.execute("DELETE FROM sync_user_theme_outbox", [])?;
        Ok(())
    }

    /// Coalesce: one pending tombstone per entry id (sync v2).
    pub fn enqueue_sync_tombstone(&self, entry_id: &str) -> Result<(), rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let at = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT OR REPLACE INTO sync_tombstone_outbox (entry_id, enqueued_at) VALUES (?1, ?2)",
            [entry_id, at.as_str()],
        )?;
        Ok(())
    }

    pub fn list_sync_tombstone_outbox(&self) -> Result<Vec<String>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT entry_id FROM sync_tombstone_outbox ORDER BY enqueued_at ASC",
        )?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect()
    }

    /// Tombstones old enough to publish. Younger than `min_age_secs` are still inside the
    /// undo window and must not reach other devices.
    pub fn list_due_sync_tombstone_outbox(&self, min_age_secs: i64) -> Result<Vec<String>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let cutoff = (chrono::Utc::now() - chrono::Duration::seconds(min_age_secs)).to_rfc3339();
        let mut stmt = conn.prepare(
            "SELECT entry_id FROM sync_tombstone_outbox WHERE enqueued_at <= ?1 ORDER BY enqueued_at ASC",
        )?;
        let rows = stmt.query_map([cutoff], |r| r.get(0))?;
        rows.collect()
    }

    pub fn remove_sync_tombstone_outbox(&self, entry_id: &str) -> Result<(), rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        conn.execute(
            "DELETE FROM sync_tombstone_outbox WHERE entry_id = ?1",
            [entry_id],
        )?;
        Ok(())
    }

    /// Clears pending Firestore tombstone writes when the cloud session is gone (e.g. account deleted elsewhere).
    pub fn clear_sync_tombstone_outbox_all(&self) -> Result<(), rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        conn.execute("DELETE FROM sync_tombstone_outbox", [])?;
        Ok(())
    }

    pub fn clear_firestore_ingest_suppression(&self, entry_id: &str) -> Result<(), rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        conn.execute(
            "DELETE FROM firestore_ingest_suppressed_ids WHERE id = ?1",
            [entry_id],
        )?;
        Ok(())
    }

    pub fn enqueue_sync_user_theme_upsert(
        &self,
        theme_id: &str,
        label: &str,
        sort_order: i32,
    ) -> Result<(), rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let at = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT OR REPLACE INTO sync_user_theme_outbox (theme_id, op, label, sort_order, enqueued_at)
             VALUES (?1, 'upsert', ?2, ?3, ?4)",
            rusqlite::params![theme_id, label, sort_order, at],
        )?;
        Ok(())
    }

    pub fn enqueue_sync_user_theme_tombstone(&self, theme_id: &str) -> Result<(), rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let at = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT OR REPLACE INTO sync_user_theme_outbox (theme_id, op, label, sort_order, enqueued_at)
             VALUES (?1, 'tombstone', NULL, NULL, ?2)",
            rusqlite::params![theme_id, at],
        )?;
        Ok(())
    }

    pub fn list_sync_user_theme_outbox(
        &self,
    ) -> Result<Vec<(String, String, Option<String>, Option<i32>)>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT theme_id, op, label, sort_order FROM sync_user_theme_outbox ORDER BY enqueued_at ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
            ))
        })?;
        rows.collect()
    }

    pub fn remove_sync_user_theme_outbox(&self, theme_id: &str) -> Result<(), rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        conn.execute(
            "DELETE FROM sync_user_theme_outbox WHERE theme_id = ?1",
            [theme_id],
        )?;
        Ok(())
    }

    pub fn clear_sync_user_theme_outbox_all(&self) -> Result<(), rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        conn.execute("DELETE FROM sync_user_theme_outbox", [])?;
        Ok(())
    }

    pub fn enqueue_all_local_user_themes_for_sync(&self) -> Result<(), rusqlite::Error> {
        let themes = self.list_user_themes()?;
        for theme in themes {
            self.enqueue_sync_user_theme_upsert(&theme.id, &theme.label, theme.sort_order)?;
        }
        Ok(())
    }

    pub fn list_entry_ids_with_themes(&self) -> Result<Vec<String>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare("SELECT entry_id FROM entry_themes")?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        rows.collect()
    }

    pub fn add_user_theme_ingest_suppression(&self, theme_id: &str) -> Result<(), rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let at = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT OR REPLACE INTO firestore_ingest_suppressed_theme_ids (id, suppressed_at) VALUES (?1, ?2)",
            [theme_id, at.as_str()],
        )?;
        Ok(())
    }

    pub fn clear_user_theme_ingest_suppression(&self, theme_id: &str) -> Result<(), rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        conn.execute(
            "DELETE FROM firestore_ingest_suppressed_theme_ids WHERE id = ?1",
            [theme_id],
        )?;
        Ok(())
    }

    pub fn apply_remote_entry_theme(
        &self,
        entry_id: &str,
        theme: Option<&EntryThemeRow>,
    ) -> Result<bool, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let locked: Option<i32> = conn
            .query_row(
                "SELECT locked FROM entry_themes WHERE entry_id = ?1",
                [entry_id],
                |row| row.get(0),
            )
            .ok();
        if locked.unwrap_or(0) != 0 {
            return Ok(false);
        }
        if theme.is_none() {
            let n = conn.execute(
                "DELETE FROM entry_themes WHERE entry_id = ?1 AND locked = 0",
                [entry_id],
            )?;
            return Ok(n > 0);
        }
        let theme = theme.unwrap();
        let classified_at = chrono::Utc::now().to_rfc3339();
        let locked_i = if theme.locked { 1 } else { 0 };
        let n = conn.execute(
            "INSERT INTO entry_themes (entry_id, theme_id, confidence, source, locked, classified_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(entry_id) DO UPDATE SET
               theme_id = excluded.theme_id,
               confidence = excluded.confidence,
               source = excluded.source,
               locked = excluded.locked,
               classified_at = excluded.classified_at
             WHERE locked = 0",
            rusqlite::params![
                entry_id,
                theme.theme_id,
                theme.confidence,
                theme.source,
                locked_i,
                classified_at
            ],
        )?;
        Ok(n > 0)
    }

    pub fn ingest_remote_user_themes(
        &self,
        rows: &[(String, String, i32)],
    ) -> Result<u32, rusqlite::Error> {
        if rows.is_empty() {
            return Ok(0);
        }
        let conn = self.0.lock().unwrap();
        let mut applied: u32 = 0;
        for (id, label, sort_order) in rows {
            let label = label.trim();
            if label.is_empty() {
                continue;
            }
            let suppressed: i32 = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM firestore_ingest_suppressed_theme_ids WHERE id = ?1)",
                [id.as_str()],
                |r| r.get(0),
            )?;
            if suppressed != 0 {
                continue;
            }
            let created_at = chrono::Utc::now().to_rfc3339();
            let n = conn.execute(
                "INSERT INTO user_themes (id, label, keywords, sort_order, created_at)
                 VALUES (?1, ?2, '[]', ?3, ?4)
                 ON CONFLICT(id) DO UPDATE SET label = excluded.label, sort_order = excluded.sort_order",
                rusqlite::params![id.as_str(), label, sort_order, created_at],
            )?;
            if n > 0 {
                applied += 1;
            }
        }
        Ok(applied)
    }

    pub fn apply_remote_user_theme_tombstones(&self, theme_ids: &[String]) -> Result<u32, rusqlite::Error> {
        if theme_ids.is_empty() {
            return Ok(0);
        }
        let conn = self.0.lock().unwrap();
        let mut removed: u32 = 0;
        for theme_id in theme_ids {
            let suppressed: i32 = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM firestore_ingest_suppressed_theme_ids WHERE id = ?1)",
                [theme_id.as_str()],
                |r| r.get(0),
            )?;
            if suppressed != 0 {
                continue;
            }
            conn.execute("DELETE FROM entry_themes WHERE theme_id = ?1", [theme_id.as_str()])?;
            let n = conn.execute("DELETE FROM user_themes WHERE id = ?1", [theme_id.as_str()])?;
            if n > 0 {
                removed += 1;
            }
            conn.execute(
                "DELETE FROM firestore_ingest_suppressed_theme_ids WHERE id = ?1",
                [theme_id.as_str()],
            )?;
        }
        Ok(removed)
    }

    /// Remote tombstone (or sync apply): remove local row without adding suppression; clear suppression for this id.
    pub fn delete_local_entries_for_sync(&self, entry_ids: &[String]) -> Result<u32, rusqlite::Error> {
        if entry_ids.is_empty() {
            return Ok(0);
        }
        let conn = self.0.lock().unwrap();
        let mut removed: u32 = 0;
        for id in entry_ids {
            let n = conn.execute("DELETE FROM entries WHERE id = ?1", [id.as_str()])?;
            if n > 0 {
                removed += 1;
            }
            conn.execute(
                "DELETE FROM firestore_ingest_suppressed_ids WHERE id = ?1",
                [id.as_str()],
            )?;
        }
        Ok(removed)
    }

    pub fn list_theme_counts(
        &self,
        min_confidence: f64,
    ) -> Result<Vec<(String, i64)>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT theme_id, COUNT(*) FROM entry_themes
             WHERE locked = 1 OR confidence >= ?1
             GROUP BY theme_id",
        )?;
        let rows = stmt.query_map([min_confidence], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;
        rows.collect()
    }

    pub fn list_theme_counts_since(
        &self,
        min_confidence: f64,
        since_rfc3339: &str,
    ) -> Result<Vec<(String, i64)>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT et.theme_id, COUNT(*) FROM entries e
             INNER JOIN entry_themes et ON et.entry_id = e.id
             WHERE (et.locked = 1 OR et.confidence >= ?1)
             AND e.created_at >= ?2
             GROUP BY et.theme_id",
        )?;
        let rows = stmt.query_map(rusqlite::params![min_confidence, since_rfc3339], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;
        rows.collect()
    }

    pub fn list_entries_filtered_by_theme(
        &self,
        theme_id: &str,
        filter: &SpaceFilter,
    ) -> Result<Vec<SearchEntryRow>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let min = THEME_RECALL_MIN_CONFIDENCE;
        let sql = match filter {
            SpaceFilter::All => format!(
                "SELECT e.id, e.text, e.created_at, e.space_id FROM entries e
                 INNER JOIN entry_themes et ON et.entry_id = e.id
                   AND (et.locked = 1 OR et.confidence >= {min})
                   AND et.theme_id = ?1
                 ORDER BY e.created_at DESC"
            ),
            SpaceFilter::Inbox => format!(
                "SELECT e.id, e.text, e.created_at, e.space_id FROM entries e
                 INNER JOIN entry_themes et ON et.entry_id = e.id
                   AND (et.locked = 1 OR et.confidence >= {min})
                   AND et.theme_id = ?1
                 WHERE e.space_id IS NULL
                 ORDER BY e.created_at DESC"
            ),
            SpaceFilter::Space(_) => format!(
                "SELECT e.id, e.text, e.created_at, e.space_id FROM entries e
                 INNER JOIN entry_themes et ON et.entry_id = e.id
                   AND (et.locked = 1 OR et.confidence >= {min})
                   AND et.theme_id = ?1
                 WHERE e.space_id = ?2
                 ORDER BY e.created_at DESC"
            ),
        };
        let mut stmt = conn.prepare(&sql)?;
        let rows = match filter {
            SpaceFilter::All | SpaceFilter::Inbox => {
                stmt.query_map([theme_id], search_row_plain)?
            }
            SpaceFilter::Space(sid) => stmt.query_map(rusqlite::params![theme_id, sid.as_str()], search_row_plain)?,
        };
        rows.collect()
    }

    pub fn search_entries_filtered(
        &self,
        query: &str,
        filter: &SpaceFilter,
        theme_id: Option<&str>,
    ) -> Result<Vec<SearchEntryRow>, rusqlite::Error> {
        if query.trim().is_empty() {
            if let Some(tid) = theme_id {
                return self.list_entries_filtered_by_theme(tid, filter);
            }
            let rows = self.list_entries_filtered(filter)?;
            return Ok(rows
                .into_iter()
                .map(|r| SearchEntryRow {
                    id: r.id,
                    text: r.text,
                    created_at: r.created_at,
                    highlighted: None,
                    space_id: r.space_id,
                })
                .collect());
        }
        if let Some(tid) = theme_id {
            return self.search_entries_with_theme(query, filter, tid);
        }
        let conn = self.0.lock().unwrap();
        let prefix_query = fts5_prefix_query(query);

        if !prefix_query.is_empty() {
            let fts_sql_all = "SELECT e.id, e.text, e.created_at, e.space_id,
                    highlight(entries_fts, 0, '\u{0001}', '\u{0002}') AS highlighted
             FROM entries e
             INNER JOIN entries_fts ON e.rowid = entries_fts.rowid
             WHERE entries_fts MATCH ?1
             ORDER BY bm25(entries_fts)";
            let fts_sql_inbox = "SELECT e.id, e.text, e.created_at, e.space_id,
                    highlight(entries_fts, 0, '\u{0001}', '\u{0002}') AS highlighted
             FROM entries e
             INNER JOIN entries_fts ON e.rowid = entries_fts.rowid
             WHERE entries_fts MATCH ?1 AND e.space_id IS NULL
             ORDER BY bm25(entries_fts)";
            let fts_sql_space = "SELECT e.id, e.text, e.created_at, e.space_id,
                    highlight(entries_fts, 0, '\u{0001}', '\u{0002}') AS highlighted
             FROM entries e
             INNER JOIN entries_fts ON e.rowid = entries_fts.rowid
             WHERE entries_fts MATCH ?1 AND e.space_id = ?2
             ORDER BY bm25(entries_fts)";

            let fts_list: Option<Vec<SearchEntryRow>> = match filter {
                SpaceFilter::All => conn.prepare(fts_sql_all).ok().and_then(|mut stmt| {
                    stmt
                        .query_map([&prefix_query], fts_match_row)
                        .ok()
                        .and_then(|rows| rows.collect::<Result<Vec<_>, _>>().ok())
                }),
                SpaceFilter::Inbox => conn.prepare(fts_sql_inbox).ok().and_then(|mut stmt| {
                    stmt
                        .query_map([&prefix_query], fts_match_row)
                        .ok()
                        .and_then(|rows| rows.collect::<Result<Vec<_>, _>>().ok())
                }),
                SpaceFilter::Space(sid) => conn.prepare(fts_sql_space).ok().and_then(|mut stmt| {
                    stmt
                        .query_map(rusqlite::params![prefix_query, sid.as_str()], fts_match_row)
                        .ok()
                        .and_then(|rows| rows.collect::<Result<Vec<_>, _>>().ok())
                }),
            };
            if let Some(list) = fts_list {
                if !list.is_empty() {
                    return Ok(list);
                }
            }
        }

        let query_trim = query.trim();
        let like_pattern = format!("%{}%", escape_like(query_trim));
        match filter {
            SpaceFilter::All => {
                let fallback_sql = "SELECT id, text, created_at, space_id FROM entries WHERE LOWER(text) LIKE LOWER(?1) ESCAPE '\\' ORDER BY created_at DESC";
                let mut stmt = conn.prepare(fallback_sql)?;
                let rows = stmt.query_map([&like_pattern], |r| {
                    let id: String = r.get(0)?;
                    let text: String = r.get(1)?;
                    let created_at: String = r.get(2)?;
                    let space_id: Option<String> = r.get(3)?;
                    let highlighted = Some(highlight_substring(&text, query_trim));
                    Ok(SearchEntryRow {
                        id,
                        text,
                        created_at,
                        space_id,
                        highlighted,
                    })
                })?;
                rows.collect()
            }
            SpaceFilter::Inbox => {
                let fallback_sql = "SELECT id, text, created_at, space_id FROM entries WHERE LOWER(text) LIKE LOWER(?1) ESCAPE '\\' AND space_id IS NULL ORDER BY created_at DESC";
                let mut stmt = conn.prepare(fallback_sql)?;
                let rows = stmt.query_map([&like_pattern], |r| {
                    let id: String = r.get(0)?;
                    let text: String = r.get(1)?;
                    let created_at: String = r.get(2)?;
                    let space_id: Option<String> = r.get(3)?;
                    let highlighted = Some(highlight_substring(&text, query_trim));
                    Ok(SearchEntryRow {
                        id,
                        text,
                        created_at,
                        space_id,
                        highlighted,
                    })
                })?;
                rows.collect()
            }
            SpaceFilter::Space(sid) => {
                let fallback_sql = "SELECT id, text, created_at, space_id FROM entries WHERE LOWER(text) LIKE LOWER(?1) ESCAPE '\\' AND space_id = ?2 ORDER BY created_at DESC";
                let mut stmt = conn.prepare(fallback_sql)?;
                let rows =
                    stmt.query_map(rusqlite::params![like_pattern, sid.as_str()], |r| {
                        let id: String = r.get(0)?;
                        let text: String = r.get(1)?;
                        let created_at: String = r.get(2)?;
                        let space_id: Option<String> = r.get(3)?;
                        let highlighted = Some(highlight_substring(&text, query_trim));
                        Ok(SearchEntryRow {
                            id,
                            text,
                            created_at,
                            space_id,
                            highlighted,
                        })
                    })?;
                rows.collect()
            }
        }
    }

    fn search_entries_with_theme(
        &self,
        query: &str,
        filter: &SpaceFilter,
        theme_id: &str,
    ) -> Result<Vec<SearchEntryRow>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let min = THEME_RECALL_MIN_CONFIDENCE;
        let theme_join = format!(
            "INNER JOIN entry_themes et ON et.entry_id = e.id
             AND (et.locked = 1 OR et.confidence >= {min})"
        );
        let prefix_query = fts5_prefix_query(query);
        if !prefix_query.is_empty() {
            let fts_sql_all = format!(
                "SELECT e.id, e.text, e.created_at, e.space_id,
                        highlight(entries_fts, 0, '\u{0001}', '\u{0002}') AS highlighted
                 FROM entries e
                 {theme_join}
                 INNER JOIN entries_fts ON e.rowid = entries_fts.rowid
                 WHERE entries_fts MATCH ?1 AND et.theme_id = ?2
                 ORDER BY bm25(entries_fts)"
            );
            let fts_sql_inbox = format!(
                "SELECT e.id, e.text, e.created_at, e.space_id,
                        highlight(entries_fts, 0, '\u{0001}', '\u{0002}') AS highlighted
                 FROM entries e
                 {theme_join}
                 INNER JOIN entries_fts ON e.rowid = entries_fts.rowid
                 WHERE entries_fts MATCH ?1 AND e.space_id IS NULL AND et.theme_id = ?2
                 ORDER BY bm25(entries_fts)"
            );
            let fts_sql_space = format!(
                "SELECT e.id, e.text, e.created_at, e.space_id,
                        highlight(entries_fts, 0, '\u{0001}', '\u{0002}') AS highlighted
                 FROM entries e
                 {theme_join}
                 INNER JOIN entries_fts ON e.rowid = entries_fts.rowid
                 WHERE entries_fts MATCH ?1 AND e.space_id = ?2 AND et.theme_id = ?3
                 ORDER BY bm25(entries_fts)"
            );
            let fts_list: Option<Vec<SearchEntryRow>> = match filter {
                SpaceFilter::All => conn.prepare(&fts_sql_all).ok().and_then(|mut stmt| {
                    stmt
                        .query_map(rusqlite::params![prefix_query, theme_id], fts_match_row)
                        .ok()
                        .and_then(|rows| rows.collect::<Result<Vec<_>, _>>().ok())
                }),
                SpaceFilter::Inbox => conn.prepare(&fts_sql_inbox).ok().and_then(|mut stmt| {
                    stmt
                        .query_map(rusqlite::params![prefix_query, theme_id], fts_match_row)
                        .ok()
                        .and_then(|rows| rows.collect::<Result<Vec<_>, _>>().ok())
                }),
                SpaceFilter::Space(sid) => conn.prepare(&fts_sql_space).ok().and_then(|mut stmt| {
                    stmt
                        .query_map(
                            rusqlite::params![prefix_query, sid.as_str(), theme_id],
                            fts_match_row,
                        )
                        .ok()
                        .and_then(|rows| rows.collect::<Result<Vec<_>, _>>().ok())
                }),
            };
            if let Some(list) = fts_list {
                if !list.is_empty() {
                    return Ok(list);
                }
            }
        }

        let query_trim = query.trim();
        let like_pattern = format!("%{}%", escape_like(query_trim));
        match filter {
            SpaceFilter::All => {
                let fallback_sql = format!(
                    "SELECT e.id, e.text, e.created_at, e.space_id FROM entries e
                     {theme_join}
                     WHERE LOWER(e.text) LIKE LOWER(?1) ESCAPE '\\' AND et.theme_id = ?2
                     ORDER BY e.created_at DESC"
                );
                let mut stmt = conn.prepare(&fallback_sql)?;
                let rows = stmt.query_map(rusqlite::params![like_pattern, theme_id], |r| {
                    let id: String = r.get(0)?;
                    let text: String = r.get(1)?;
                    let created_at: String = r.get(2)?;
                    let space_id: Option<String> = r.get(3)?;
                    let highlighted = Some(highlight_substring(&text, query_trim));
                    Ok(SearchEntryRow {
                        id,
                        text,
                        created_at,
                        space_id,
                        highlighted,
                    })
                })?;
                rows.collect()
            }
            SpaceFilter::Inbox => {
                let fallback_sql = format!(
                    "SELECT e.id, e.text, e.created_at, e.space_id FROM entries e
                     {theme_join}
                     WHERE LOWER(e.text) LIKE LOWER(?1) ESCAPE '\\' AND e.space_id IS NULL AND et.theme_id = ?2
                     ORDER BY e.created_at DESC"
                );
                let mut stmt = conn.prepare(&fallback_sql)?;
                let rows = stmt.query_map(rusqlite::params![like_pattern, theme_id], |r| {
                    let id: String = r.get(0)?;
                    let text: String = r.get(1)?;
                    let created_at: String = r.get(2)?;
                    let space_id: Option<String> = r.get(3)?;
                    let highlighted = Some(highlight_substring(&text, query_trim));
                    Ok(SearchEntryRow {
                        id,
                        text,
                        created_at,
                        space_id,
                        highlighted,
                    })
                })?;
                rows.collect()
            }
            SpaceFilter::Space(sid) => {
                let fallback_sql = format!(
                    "SELECT e.id, e.text, e.created_at, e.space_id FROM entries e
                     {theme_join}
                     WHERE LOWER(e.text) LIKE LOWER(?1) ESCAPE '\\' AND e.space_id = ?2 AND et.theme_id = ?3
                     ORDER BY e.created_at DESC"
                );
                let mut stmt = conn.prepare(&fallback_sql)?;
                let rows = stmt.query_map(
                    rusqlite::params![like_pattern, sid.as_str(), theme_id],
                    |r| {
                        let id: String = r.get(0)?;
                        let text: String = r.get(1)?;
                        let created_at: String = r.get(2)?;
                        let space_id: Option<String> = r.get(3)?;
                        let highlighted = Some(highlight_substring(&text, query_trim));
                        Ok(SearchEntryRow {
                            id,
                            text,
                            created_at,
                            space_id,
                            highlighted,
                        })
                    },
                )?;
                rows.collect()
            }
        }
    }

    pub fn insert_share_thread(
        &self,
        token: &str,
        entry_ids: &[String],
        context_note: Option<&str>,
        created_at: &str,
        expires_at: &str,
    ) -> Result<(), rusqlite::Error> {
        let entry_ids_json = serde_json::to_string(entry_ids).map_err(|e| {
            rusqlite::Error::ToSqlConversionFailure(Box::new(e))
        })?;
        let conn = self.0.lock().unwrap();
        conn.execute(
            "INSERT INTO share_threads (token, entry_ids, context_note, created_at, expires_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![token, entry_ids_json, context_note, created_at, expires_at],
        )?;
        Ok(())
    }

    pub fn get_share_thread_row(
        &self,
        token: &str,
    ) -> Result<Option<ShareThreadRow>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT token, entry_ids, context_note, created_at, expires_at, revoked_at FROM share_threads WHERE token = ?1",
        )?;
        let mut rows = stmt.query([token])?;
        if let Some(row) = rows.next()? {
            return Ok(Some(row_to_share_thread(&row)?));
        }
        Ok(None)
    }

    pub fn list_share_thread_rows(&self) -> Result<Vec<ShareThreadRow>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT token, entry_ids, context_note, created_at, expires_at, revoked_at FROM share_threads ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], row_to_share_thread)?;
        rows.collect()
    }

    pub fn revoke_share_thread(&self, token: &str) -> Result<bool, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let revoked_at = chrono::Utc::now().to_rfc3339();
        let n = conn.execute(
            "UPDATE share_threads SET revoked_at = ?1 WHERE token = ?2 AND revoked_at IS NULL",
            rusqlite::params![revoked_at, token],
        )?;
        Ok(n > 0)
    }
}

pub const MAX_SHARE_ENTRY_COUNT: usize = 15;

#[derive(Clone, Debug)]
pub struct ShareThreadRow {
    pub token: String,
    pub entry_ids: Vec<String>,
    pub context_note: Option<String>,
    pub created_at: String,
    pub expires_at: String,
    pub revoked_at: Option<String>,
}

pub fn share_thread_is_active(row: &ShareThreadRow) -> bool {
    if row.revoked_at.is_some() {
        return false;
    }
    let Ok(expires) = chrono::DateTime::parse_from_rfc3339(&row.expires_at) else {
        return false;
    };
    expires > chrono::Utc::now()
}

fn row_to_share_thread(r: &rusqlite::Row<'_>) -> Result<ShareThreadRow, rusqlite::Error> {
    let entry_ids_json: String = r.get(1)?;
    let entry_ids: Vec<String> = serde_json::from_str(&entry_ids_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            Box::new(e),
        )
    })?;
    Ok(ShareThreadRow {
        token: r.get(0)?,
        entry_ids,
        context_note: r.get(2)?,
        created_at: r.get(3)?,
        expires_at: r.get(4)?,
        revoked_at: r.get(5)?,
    })
}

fn row_to_entry(r: &rusqlite::Row<'_>) -> Result<EntryRow, rusqlite::Error> {
    Ok(EntryRow {
        id: r.get(0)?,
        text: r.get(1)?,
        created_at: r.get(2)?,
        updated_at: r.get(3)?,
        edit_count: r.get::<_, i64>(4)? as u32,
        open_count: r.get::<_, i64>(5)? as u32,
        space_id: r.get(6)?,
        continuation_from: r.get(7)?,
        continuation_at: r.get(8)?,
    })
}

#[derive(Clone)]
pub struct EntryRow {
    pub id: String,
    pub text: String,
    pub created_at: String,
    pub updated_at: String,
    pub edit_count: u32,
    pub open_count: u32,
    pub space_id: Option<String>,
    pub continuation_from: Option<i32>,
    pub continuation_at: Option<String>,
}

pub struct SearchEntryRow {
    pub id: String,
    pub text: String,
    pub created_at: String,
    pub highlighted: Option<String>,
    pub space_id: Option<String>,
}

fn search_row_plain(r: &rusqlite::Row<'_>) -> Result<SearchEntryRow, rusqlite::Error> {
    Ok(SearchEntryRow {
        id: r.get(0)?,
        text: r.get(1)?,
        created_at: r.get(2)?,
        space_id: r.get(3)?,
        highlighted: None,
    })
}

fn fts_match_row(r: &rusqlite::Row<'_>) -> Result<SearchEntryRow, rusqlite::Error> {
    Ok(SearchEntryRow {
        id: r.get(0)?,
        text: r.get(1)?,
        created_at: r.get(2)?,
        space_id: r.get(3)?,
        highlighted: Some(r.get(4)?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn db_with_entries(entries: &[(&str, &str)]) -> Db {
        let db = Db::open(PathBuf::from(":memory:")).unwrap();
        for (i, (id, text)) in entries.iter().enumerate() {
            let created = format!("2025-01-{:02}T12:00:00Z", 15 - i);
            db.create_entry(id, text, &created, None).unwrap();
        }
        db
    }

    fn search_ids(db: &Db, query: &str) -> Vec<String> {
        db.search_entries_filtered(query, &SpaceFilter::All, None)
            .unwrap()
            .into_iter()
            .map(|r| r.id.to_string())
            .collect()
    }

    #[test]
    fn partial_word_search_matches_substrings_inside_words() {
        let db = db_with_entries(&[(
            "1",
            "1:1 with Sarah re: roadmap alignment and hiring plan for Q2",
        )]);
        for query in ["road", "roadmap", "align", "hir", "plan"] {
            let ids = search_ids(&db, query);
            assert!(
                ids.contains(&"1".to_string()),
                "query {:?} should match entry (substring inside word)",
                query
            );
        }
    }

    #[test]
    fn case_insensitive_matching() {
        let db = db_with_entries(&[("1", "Call Mom Tomorrow")]);
        for query in ["call", "CALL", "Call", "mom", "MOM", "ToMoRrOw"] {
            let ids = search_ids(&db, query);
            assert!(
                ids.contains(&"1".to_string()),
                "query {:?} should match (case-insensitive)",
                query
            );
        }
    }

    #[test]
    fn no_results_returns_empty() {
        let db = db_with_entries(&[("1", "1:1 with Sarah re: roadmap alignment")]);
        let ids = search_ids(&db, "xyznonexistent");
        assert!(ids.is_empty(), "query with no matches should return empty");
    }

    #[test]
    fn firestore_ingest_skips_duplicate_id() {
        let db = Db::open(PathBuf::from(":memory:")).unwrap();
        db.create_entry("a", "local", "2025-01-01T12:00:00Z", None)
            .unwrap();
        let n = db
            .ingest_firestore_entries(&[(
                "a".to_string(),
                "remote".to_string(),
                "2025-02-01T12:00:00Z".to_string(),
            )])
            .unwrap();
        assert_eq!(n, 0);
        let row = db.get_entry_by_id("a").unwrap().unwrap();
        assert_eq!(row.text, "local");
    }

    #[test]
    fn firestore_ingest_inserts_new_id() {
        let db = Db::open(PathBuf::from(":memory:")).unwrap();
        let n = db
            .ingest_firestore_entries(&[(
                "x".to_string(),
                "from cloud".to_string(),
                "2025-03-10T08:30:00.000Z".to_string(),
            )])
            .unwrap();
        assert_eq!(n, 1);
        let row = db.get_entry_by_id("x").unwrap().unwrap();
        assert_eq!(row.text, "from cloud");
    }

    #[test]
    fn firestore_ingest_skips_desktop_deleted_id() {
        let db = Db::open(PathBuf::from(":memory:")).unwrap();
        db.create_entry("meow", "cat", "2025-01-01T12:00:00Z", None).unwrap();
        db.delete_entry("meow").unwrap();
        assert!(db.get_entry_by_id("meow").unwrap().is_none());
        let n = db
            .ingest_firestore_entries(&[(
                "meow".to_string(),
                "still in cloud".to_string(),
                "2025-02-01T12:00:00Z".to_string(),
            )])
            .unwrap();
        assert_eq!(n, 0);
        assert!(db.get_entry_by_id("meow").unwrap().is_none());
    }

    #[test]
    fn create_entry_clears_firestore_ingest_suppression() {
        let db = Db::open(PathBuf::from(":memory:")).unwrap();
        db.create_entry("r", "one", "2025-01-01T12:00:00Z", None).unwrap();
        db.delete_entry("r").unwrap();
        db.create_entry("r", "two", "2025-01-02T12:00:00Z", None).unwrap();
        let n = db
            .ingest_firestore_entries(&[(
                "r".to_string(),
                "from cloud".to_string(),
                "2025-03-01T12:00:00Z".to_string(),
            )])
            .unwrap();
        assert_eq!(n, 0);
        let row = db.get_entry_by_id("r").unwrap().unwrap();
        assert_eq!(row.text, "two");
    }

    #[test]
    fn basic_ranking_stronger_match_first() {
        let db = db_with_entries(&[
            ("once", "design review notes"),
            ("twice", "design and design again"),
        ]);
        let rows = db
            .search_entries_filtered("design", &SpaceFilter::All, None)
            .unwrap();
        assert_eq!(rows.len(), 2);
        let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(
            ids[0], "twice",
            "entry with more term matches should rank higher"
        );
        assert_eq!(ids[1], "once");
    }

    #[test]
    fn sync_tombstone_outbox_coalesces_same_entry_id() {
        let db = Db::open(PathBuf::from(":memory:")).unwrap();
        db.enqueue_sync_tombstone("same").unwrap();
        db.enqueue_sync_tombstone("same").unwrap();
        let ids = db.list_sync_tombstone_outbox().unwrap();
        assert_eq!(ids, vec!["same".to_string()]);
    }

    #[test]
    fn a_fresh_tombstone_is_not_due_for_eight_seconds() {
        let db = Db::open(PathBuf::from(":memory:")).unwrap();
        db.enqueue_sync_tombstone("fresh").unwrap();
        assert!(
            db.list_due_sync_tombstone_outbox(8).unwrap().is_empty(),
            "publishing inside the undo window would destroy the fragment on other devices"
        );
        assert_eq!(db.list_sync_tombstone_outbox().unwrap(), vec!["fresh".to_string()]);
    }

    #[test]
    fn an_old_enough_tombstone_is_due() {
        let db = Db::open(PathBuf::from(":memory:")).unwrap();
        {
            let conn = db.0.lock().unwrap();
            conn.execute(
                "INSERT INTO sync_tombstone_outbox (entry_id, enqueued_at) VALUES ('old', '2020-01-01T00:00:00+00:00')",
                [],
            )
            .unwrap();
        }
        assert_eq!(db.list_due_sync_tombstone_outbox(8).unwrap(), vec!["old".to_string()]);
    }

    #[test]
    fn delete_local_entries_for_sync_clears_suppression() {
        let db = Db::open(PathBuf::from(":memory:")).unwrap();
        db.create_entry("gone", "x", "2025-01-01T12:00:00Z", None).unwrap();
        db.delete_entry("gone").unwrap();
        assert!(db.get_entry_by_id("gone").unwrap().is_none());
        let n = db
            .ingest_firestore_entries(&[(
                "gone".to_string(),
                "cloud".to_string(),
                "2025-02-01T12:00:00Z".to_string(),
            )])
            .unwrap();
        assert_eq!(n, 0);
        let removed = db
            .delete_local_entries_for_sync(&["gone".to_string()])
            .unwrap();
        assert_eq!(removed, 0);
        let n2 = db
            .ingest_firestore_entries(&[(
                "gone".to_string(),
                "cloud".to_string(),
                "2025-02-01T12:00:00Z".to_string(),
            )])
            .unwrap();
        assert_eq!(n2, 1);
        assert_eq!(
            db.get_entry_by_id("gone").unwrap().unwrap().text,
            "cloud"
        );
    }

    #[test]
    fn jump_anchor_matches_list_entries_local_day() {
        let db = Db::open(PathBuf::from(":memory:")).unwrap();
        db.create_entry("e1", "x", "2025-01-20T18:30:00Z", None).unwrap();
        let created = db.list_entries().unwrap()[0].created_at.clone();
        let utc: chrono::DateTime<chrono::Utc> = chrono::DateTime::parse_from_rfc3339(&created)
            .unwrap()
            .with_timezone(&chrono::Utc);
        let target = utc.with_timezone(&chrono::Local).date_naive();
        assert_eq!(
            db.jump_anchor_entry_id_for_local_date(target, &SpaceFilter::All)
                .unwrap()
                .as_deref(),
            Some("e1")
        );
    }

    #[test]
    fn clear_firestore_ingest_suppression_allows_ingest_like_tombstone_success() {
        let db = Db::open(PathBuf::from(":memory:")).unwrap();
        db.create_entry("t", "local", "2025-01-01T12:00:00Z", None).unwrap();
        db.delete_entry("t").unwrap();
        assert_eq!(
            db.ingest_firestore_entries(&[(
                "t".to_string(),
                "remote".to_string(),
                "2025-03-01T12:00:00Z".to_string(),
            )])
            .unwrap(),
            0
        );
        db.clear_firestore_ingest_suppression("t").unwrap();
        assert_eq!(
            db.ingest_firestore_entries(&[(
                "t".to_string(),
                "remote".to_string(),
                "2025-03-01T12:00:00Z".to_string(),
            )])
            .unwrap(),
            1
        );
    }

    #[test]
    fn remove_sync_tombstone_outbox_is_idempotent() {
        let db = Db::open(PathBuf::from(":memory:")).unwrap();
        db.enqueue_sync_tombstone("z").unwrap();
        db.remove_sync_tombstone_outbox("z").unwrap();
        db.remove_sync_tombstone_outbox("z").unwrap();
        assert!(db.list_sync_tombstone_outbox().unwrap().is_empty());
    }

    #[test]
    fn clear_sync_tombstone_outbox_all_empties_queue() {
        let db = Db::open(PathBuf::from(":memory:")).unwrap();
        db.enqueue_sync_tombstone("a").unwrap();
        db.enqueue_sync_tombstone("b").unwrap();
        db.clear_sync_tombstone_outbox_all().unwrap();
        assert!(db.list_sync_tombstone_outbox().unwrap().is_empty());
    }

    #[test]
    fn create_and_update_user_theme_enqueues_sync_outbox() {
        let db = Db::open(PathBuf::from(":memory:")).unwrap();
        let created = db.create_user_theme("Ideas").unwrap();
        assert_eq!(created.label, "Ideas");
        let outbox = db.list_sync_user_theme_outbox().unwrap();
        assert_eq!(outbox.len(), 1);
        assert_eq!(outbox[0].0, created.id);
        assert_eq!(outbox[0].1, "upsert");

        let updated = db.update_user_theme(&created.id, "Notes").unwrap();
        assert_eq!(updated.label, "Notes");
        let outbox = db.list_sync_user_theme_outbox().unwrap();
        assert_eq!(outbox.len(), 1);
        assert_eq!(outbox[0].2.as_deref(), Some("Notes"));
    }

    #[test]
    fn share_thread_create_get_revoke() {
        let db = Db::open(PathBuf::from(":memory:")).unwrap();
        db.create_entry("e1", "one", "2025-01-01T12:00:00Z", None)
            .unwrap();
        db.create_entry("e2", "two", "2025-01-02T12:00:00Z", None)
            .unwrap();
        let ids = vec!["e1".to_string(), "e2".to_string()];
        let token = "test-token";
        let created = "2025-01-03T12:00:00Z";
        let expires = "2099-01-01T12:00:00Z";
        db.insert_share_thread(token, &ids, Some("note"), created, expires)
            .unwrap();
        let row = db.get_share_thread_row(token).unwrap().unwrap();
        assert_eq!(row.entry_ids, ids);
        assert_eq!(row.context_note.as_deref(), Some("note"));
        assert!(share_thread_is_active(&row));
        assert!(db.revoke_share_thread(token).unwrap());
        let after = db.get_share_thread_row(token).unwrap().unwrap();
        assert!(!share_thread_is_active(&after));
    }

    #[test]
    fn mark_entry_continuation_sets_marker_once() {
        let db = Db::open(PathBuf::from(":memory:")).unwrap();
        db.create_entry("e1", "original", "2025-01-01T12:00:00Z", None)
            .unwrap();
        db.update_entry_text("e1", "original\ncontinued").unwrap();
        let first = db
            .mark_entry_continuation("e1", 9, "original\ncontinued")
            .unwrap();
        assert!(first.is_some());
        let row = db.get_entry_by_id("e1").unwrap().unwrap();
        assert_eq!(row.continuation_from, Some(9));
        assert!(row.continuation_at.is_some());

        let second = db
            .mark_entry_continuation("e1", 9, "original\ncontinued")
            .unwrap();
        assert_eq!(second, first);
    }

    #[test]
    fn update_entry_text_keeps_continuation_at_after_later_edits() {
        let db = Db::open(PathBuf::from(":memory:")).unwrap();
        db.create_entry("e1", "hello", "2025-01-01T12:00:00Z", None)
            .unwrap();
        db.update_entry_text("e1", "hello\nmore").unwrap();
        db.mark_entry_continuation("e1", 6, "hello\nmore").unwrap();
        let first = db.get_entry_by_id("e1").unwrap().unwrap();
        let first_at = first.continuation_at.clone().unwrap();

        db.update_entry_text("e1", "hello\nmore and more").unwrap();
        let second = db.get_entry_by_id("e1").unwrap().unwrap();
        assert_eq!(second.continuation_from, Some(6));
        assert_eq!(second.continuation_at, Some(first_at));
    }

    #[test]
    fn update_entry_text_clears_invalid_continuation_marker() {
        let db = Db::open(PathBuf::from(":memory:")).unwrap();
        db.create_entry("e1", "hello", "2025-01-01T12:00:00Z", None)
            .unwrap();
        db.update_entry_text("e1", "hello\nmore").unwrap();
        db.mark_entry_continuation("e1", 6, "hello\nmore").unwrap();
        db.update_entry_text("e1", "short").unwrap();
        let row = db.get_entry_by_id("e1").unwrap().unwrap();
        assert_eq!(row.continuation_from, None);
        assert_eq!(row.continuation_at, None);
    }

    #[test]
    fn update_entry_text_changes_body_and_increments_edit_count() {
        let db = Db::open(PathBuf::from(":memory:")).unwrap();
        db.create_entry("e1", "original", "2025-01-01T12:00:00Z", None)
            .unwrap();
        let before = db.list_entries().unwrap();
        assert_eq!(before[0].text, "original");
        assert_eq!(before[0].edit_count, 0);

        db.update_entry_text("e1", "revised body").unwrap();
        let after = db.list_entries().unwrap();
        assert_eq!(after[0].text, "revised body");
        assert_eq!(after[0].edit_count, 1);
    }

    #[test]
    fn update_entry_space_changes_lens_without_touching_edit_count() {
        let db = Db::open(PathBuf::from(":memory:")).unwrap();
        db.create_entry("e1", "hello", "2025-01-01T12:00:00Z", None)
            .unwrap();
        assert_eq!(db.list_entries_filtered(&SpaceFilter::Inbox).unwrap().len(), 1);
        assert_eq!(
            db.list_entries_filtered(&SpaceFilter::Space("work".to_string()))
                .unwrap()
                .len(),
            0
        );

        db.update_entry_space("e1", Some("work")).unwrap();
        assert_eq!(db.list_entries_filtered(&SpaceFilter::Inbox).unwrap().len(), 0);
        let work = db
            .list_entries_filtered(&SpaceFilter::Space("work".to_string()))
            .unwrap();
        assert_eq!(work.len(), 1);
        assert_eq!(work[0].space_id.as_deref(), Some("work"));
        assert_eq!(work[0].edit_count, 0);

        db.update_entry_space("e1", None).unwrap();
        assert_eq!(db.list_entries_filtered(&SpaceFilter::Inbox).unwrap().len(), 1);
    }

    #[test]
    fn list_entries_respects_space_filter() {
        let db = Db::open(PathBuf::from(":memory:")).unwrap();
        db.create_entry("a", "inbox text", "2025-01-01T12:00:00Z", None)
            .unwrap();
        db.create_entry(
            "b",
            "work text",
            "2025-01-02T12:00:00Z",
            Some("work"),
        )
        .unwrap();
        assert_eq!(
            db.list_entries_filtered(&SpaceFilter::All).unwrap().len(),
            2
        );
        let inbox = db.list_entries_filtered(&SpaceFilter::Inbox).unwrap();
        assert_eq!(inbox.len(), 1);
        assert_eq!(inbox[0].id, "a");
        let work = db
            .list_entries_filtered(&SpaceFilter::Space("work".to_string()))
            .unwrap();
        assert_eq!(work.len(), 1);
        assert_eq!(work[0].id, "b");
    }

    #[test]
    fn local_entry_dates_in_month_drops_day_after_last_delete() {
        let db = Db::open(PathBuf::from(":memory:")).unwrap();
        db.create_entry("a", "one", "2025-03-24T08:00:00Z", None).unwrap();
        db.create_entry("b", "two", "2025-03-24T18:00:00Z", None).unwrap();

        let initial = db
            .local_entry_dates_in_month(2025, 3, &SpaceFilter::All)
            .unwrap();
        assert!(
            initial.iter().any(|d| d == "2025-03-24"),
            "date with entries should be marked"
        );

        db.delete_entry("a").unwrap();
        let after_first_delete = db
            .local_entry_dates_in_month(2025, 3, &SpaceFilter::All)
            .unwrap();
        assert!(
            after_first_delete.iter().any(|d| d == "2025-03-24"),
            "date should stay marked while at least one entry remains"
        );

        db.delete_entry("b").unwrap();
        let after_last_delete = db
            .local_entry_dates_in_month(2025, 3, &SpaceFilter::All)
            .unwrap();
        assert!(
            !after_last_delete.iter().any(|d| d == "2025-03-24"),
            "date should unmark after last entry is deleted"
        );
    }
}
