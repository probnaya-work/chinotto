//! The Record: capture, Continue, Correct, hold, and the Find index.
//!
//! The promise this module has to keep is narrow and absolute:
//!
//!   * Capture writes locally and cannot fail for a reason the person has to care about.
//!   * Continue creates a NEW dated fragment and leaves every earlier moment untouched.
//!   * Correct replaces the current wording and KEEPS the one it replaced, without moving
//!     the moment in time.
//!
//! Anything that reads like "update the thought" is a bug. There is no operation here that
//! silently turns what someone thought in 2021 into what they think today.

use super::Db;
use rusqlite::{Connection, OptionalExtension};

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fragment {
    pub id: String,
    pub body: String,
    pub captured_at: String,
    pub capture_method: String,
    pub capture_origin: Option<String>,
    pub corrected_at: Option<String>,
    pub correction_count: i64,
    /// v1 edited in place, so these corrections have no recoverable earlier wording.
    pub legacy_edit_count: i64,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Revision {
    pub body: String,
    pub superseded_at: String,
    pub revision_index: i64,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineMoment {
    pub fragment: Fragment,
    pub position: i64,
    /// False for the first moment: the one that was captured before anything continued it.
    pub is_continuation: bool,
}

/// How much may be kept present at once.
///
/// "Deliberately small and bounded" is the product's phrasing, and the design draws a single
/// held item. Neither gives a number; five matches what v1's pins allowed, so every migrated
/// pin fits without anything being dropped. See docs/internal/unspecified-decisions.md.
pub const MAX_HELD: i64 = 5;

const FRAGMENT_COLUMNS: &str = "id, body, captured_at, capture_method, capture_origin, \
                                corrected_at, correction_count, legacy_edit_count";

fn map_fragment(r: &rusqlite::Row) -> Result<Fragment, rusqlite::Error> {
    Ok(Fragment {
        id: r.get(0)?,
        body: r.get(1)?,
        captured_at: r.get(2)?,
        capture_method: r.get(3)?,
        capture_origin: r.get(4)?,
        corrected_at: r.get(5)?,
        correction_count: r.get(6)?,
        legacy_edit_count: r.get(7)?,
    })
}

/// Turns what a person typed into a safe FTS5 MATCH expression.
///
/// Find takes free text — `heating engineer`, `theatlantic.com`, `"sort later"` — and FTS5's
/// query language treats `.`, `-`, `:` and quotes as syntax. Every token is therefore quoted
/// as a literal string and ANDed, so no input can be a syntax error and none of it is
/// interpreted as an operator the person did not intend to write.
pub fn fts_match_query(input: &str) -> String {
    input
        .split_whitespace()
        .map(|token| format!("\"{}\"", token.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" AND ")
}

/// Rebuilds the Find row for one fragment from every table that contributes text.
///
/// Explicit rather than trigger-driven: a fragment's searchable text is assembled from four
/// tables, and a person should be able to read this function and know exactly what Find can
/// see. Called after every write that changes any of it.
pub fn sync_fts(conn: &Connection, fragment_id: &str) -> Result<(), rusqlite::Error> {
    conn.execute(
        "DELETE FROM fragments_fts WHERE fragment_id = ?1",
        [fragment_id],
    )?;

    let row: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT body, removed_at FROM fragments WHERE id = ?1",
            [fragment_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;

    let Some((body, removed_at)) = row else {
        return Ok(());
    };
    // Removed material stays in the Record but stops answering Find.
    if removed_at.is_some() {
        return Ok(());
    }

    // The transcript is searchable, but it is derived from the audio, not typed by a person,
    // so it lives in its own column and can be ranked behind body.
    let transcript: Option<String> = conn
        .query_row(
            "SELECT machine_transcript FROM voice_transcripts WHERE fragment_id = ?1",
            [fragment_id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();

    // What a source said and was called — distinct from what the person said about it.
    // Spans both the canonical encounter (the URL and anything the source supplied) and
    // whatever enrichment has managed to fetch. A title that has not arrived yet simply
    // is not searchable yet — it is never substituted for.
    let mut source = String::new();
    {
        let mut stmt = conn.prepare(
            "SELECT e.url_raw, e.selected_text, x.url_canonical, x.domain, x.title, \
                    x.site_name, x.extraction \
             FROM encounters e LEFT JOIN encounter_enrichment x ON x.encounter_id = e.id \
             WHERE e.fragment_id = ?1",
        )?;
        let mut rows = stmt.query([fragment_id])?;
        while let Some(r) = rows.next()? {
            for i in 0..7 {
                if let Some(v) = r.get::<_, Option<String>>(i)? {
                    if !v.is_empty() {
                        source.push_str(&v);
                        source.push(' ');
                    }
                }
            }
        }
    }

    conn.execute(
        "INSERT INTO fragments_fts (body, transcript, source, fragment_id) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![
            body,
            transcript.unwrap_or_default(),
            source.trim_end(),
            fragment_id
        ],
    )?;
    Ok(())
}

/// Maintains `line_index` for the chain containing `fragment_id`.
///
/// Scoped to one line rather than rebuilding the whole table, because at 50,000 fragments a
/// global rebuild on every Continue would be the sort of cost that only shows up in year three.
fn reindex_line(conn: &Connection, fragment_id: &str) -> Result<(), rusqlite::Error> {
    // Walk to the root of this chain.
    let mut root = fragment_id.to_string();
    let mut guard = 0;
    loop {
        let parent: Option<String> = conn
            .query_row(
                "SELECT continues_id FROM continuations WHERE fragment_id = ?1",
                [&root],
                |r| r.get(0),
            )
            .optional()?;
        match parent {
            Some(p) => root = p,
            None => break,
        }
        guard += 1;
        if guard > 10_000 {
            break;
        }
    }

    // Collect the chain forward from the root, depth-first.
    let mut order: Vec<String> = Vec::new();
    let mut stack = vec![root.clone()];
    let mut seen = std::collections::HashSet::new();
    while let Some(node) = stack.pop() {
        if !seen.insert(node.clone()) {
            continue;
        }
        order.push(node.clone());
        let mut stmt = conn.prepare(
            "SELECT fragment_id FROM continuations WHERE continues_id = ?1 ORDER BY linked_at, fragment_id",
        )?;
        let kids: Vec<String> = stmt
            .query_map([&node], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        for kid in kids.into_iter().rev() {
            stack.push(kid);
        }
    }

    for id in &order {
        conn.execute("DELETE FROM line_index WHERE fragment_id = ?1", [id])?;
    }
    // A single fragment that nothing continues is not a line and earns no index row.
    if order.len() < 2 {
        return Ok(());
    }
    let len = order.len() as i64;
    for (i, id) in order.iter().enumerate() {
        conn.execute(
            "INSERT INTO line_index (fragment_id, root_id, position, line_length) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, root, i as i64, len],
        )?;
    }
    Ok(())
}

impl Db {
    /// Leaves a fragment in the Record. Local, immediate, and unclassified.
    ///
    /// Takes no title, type, space, theme or destination, and must not grow one: "dinner
    /// friday" and a considered paragraph arrive through exactly this call.
    pub fn capture_fragment(
        &self,
        body: &str,
        capture_method: &str,
        capture_origin: Option<&str>,
    ) -> Result<Fragment, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO fragments (id, body, captured_at, capture_method, capture_origin) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, body, now, capture_method, capture_origin],
        )?;
        sync_fts(&conn, &id)?;
        conn.query_row(
            &format!("SELECT {FRAGMENT_COLUMNS} FROM fragments WHERE id = ?1"),
            [&id],
            map_fragment,
        )
    }

    /// Continues an earlier moment: a NEW fragment, dated now, joined by an explicit edge.
    ///
    /// The earlier fragment is not read, not rewritten, and not touched. That is the whole
    /// point of the operation, and `continue_leaves_the_earlier_moment_untouched` guards it.
    pub fn continue_fragment(
        &self,
        continues_id: &str,
        body: &str,
        capture_method: &str,
        capture_origin: Option<&str>,
        accepted_suggestion: bool,
    ) -> Result<Fragment, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO fragments (id, body, captured_at, capture_method, capture_origin) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, body, now, capture_method, capture_origin],
        )?;
        conn.execute(
            "INSERT INTO continuations (fragment_id, continues_id, linked_at, origin) \
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                id,
                continues_id,
                now,
                if accepted_suggestion { "accepted_suggestion" } else { "explicit" }
            ],
        )?;

        sync_fts(&conn, &id)?;
        reindex_line(&conn, &id)?;

        conn.query_row(
            &format!("SELECT {FRAGMENT_COLUMNS} FROM fragments WHERE id = ?1"),
            [&id],
            map_fragment,
        )
    }

    /// Links an already-captured fragment to an earlier one, without re-dating it.
    ///
    /// This is the "yes, that continues yesterday's note" path: the capture already happened
    /// and stays valid on its own, so accepting the suggestion adds a relationship and
    /// nothing else. Declining leaves a perfectly good fragment behind.
    pub fn link_continuation(
        &self,
        fragment_id: &str,
        continues_id: &str,
    ) -> Result<(), rusqlite::Error> {
        if fragment_id == continues_id {
            return Ok(());
        }
        let conn = self.0.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT OR IGNORE INTO continuations (fragment_id, continues_id, linked_at, origin) \
             VALUES (?1, ?2, ?3, 'accepted_suggestion')",
            rusqlite::params![fragment_id, continues_id, now],
        )?;
        reindex_line(&conn, fragment_id)?;
        Ok(())
    }

    /// Repairs wording. Keeps what it replaced; does not move the moment in time.
    ///
    /// Returns the number of wordings now held for this fragment. A no-op correction (the
    /// same text) records nothing, so opening a fragment and pressing save cannot
    /// manufacture a history.
    pub fn correct_fragment(&self, id: &str, body: &str) -> Result<i64, rusqlite::Error> {
        let conn = self.0.lock().unwrap();

        let previous: Option<String> = conn
            .query_row("SELECT body FROM fragments WHERE id = ?1", [id], |r| r.get(0))
            .optional()?;
        let Some(previous) = previous else {
            return Ok(0);
        };
        if previous == body {
            return conn.query_row(
                "SELECT correction_count FROM fragments WHERE id = ?1",
                [id],
                |r| r.get(0),
            );
        }

        let now = chrono::Utc::now().to_rfc3339();
        let next_index: i64 = conn.query_row(
            "SELECT COALESCE(MAX(revision_index), -1) + 1 FROM fragment_revisions WHERE fragment_id = ?1",
            [id],
            |r| r.get(0),
        )?;

        // The wording being replaced is written down BEFORE the replacement lands.
        conn.execute(
            "INSERT INTO fragment_revisions (fragment_id, body, superseded_at, revision_index) \
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, previous, now, next_index],
        )?;
        // captured_at is deliberately absent from this UPDATE.
        conn.execute(
            "UPDATE fragments SET body = ?1, corrected_at = ?2, correction_count = correction_count + 1 \
             WHERE id = ?3",
            rusqlite::params![body, now, id],
        )?;

        sync_fts(&conn, id)?;
        // The stored vector described the wording that was just replaced, so it is now a
        // description of something that no longer exists. Dropping it makes the next
        // embed_pending pass recompute it.
        conn.execute("DELETE FROM fragment_embeddings WHERE fragment_id = ?1", [id])?;
        conn.query_row(
            "SELECT correction_count FROM fragments WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
    }

    /// One fragment by id, or None when it is not there.
    pub fn fragment(&self, id: &str) -> Result<Option<Fragment>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        conn.query_row(
            &format!("SELECT {FRAGMENT_COLUMNS} FROM fragments WHERE id = ?1"),
            [id],
            map_fragment,
        )
        .optional()
    }

    /// Every wording this fragment has had, oldest first, ending with the one it has now.
    pub fn fragment_history(&self, id: &str) -> Result<Vec<Revision>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT body, superseded_at, revision_index FROM fragment_revisions \
             WHERE fragment_id = ?1 ORDER BY revision_index",
        )?;
        let mut out: Vec<Revision> = stmt
            .query_map([id], |r| {
                Ok(Revision {
                    body: r.get(0)?,
                    superseded_at: r.get(1)?,
                    revision_index: r.get(2)?,
                })
            })?
            .collect::<Result<_, _>>()?;

        if let Some((body, corrected_at)) = conn
            .query_row(
                "SELECT body, corrected_at FROM fragments WHERE id = ?1",
                [id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
            )
            .optional()?
        {
            let next = out.len() as i64;
            out.push(Revision {
                body,
                superseded_at: corrected_at.unwrap_or_default(),
                revision_index: next,
            });
        }
        Ok(out)
    }

    /// The whole Line containing this fragment, first moment forward.
    ///
    /// A fragment nothing has continued returns just itself: a Line is what a fragment
    /// becomes, not a container it was filed into.
    pub fn line_for(&self, id: &str) -> Result<Vec<LineMoment>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();

        let root: Option<String> = conn
            .query_row(
                "SELECT root_id FROM line_index WHERE fragment_id = ?1",
                [id],
                |r| r.get(0),
            )
            .optional()?;

        let Some(root) = root else {
            let solo = conn
                .query_row(
                    &format!("SELECT {FRAGMENT_COLUMNS} FROM fragments WHERE id = ?1"),
                    [id],
                    map_fragment,
                )
                .optional()?;
            return Ok(solo
                .map(|fragment| {
                    vec![LineMoment {
                        fragment,
                        position: 0,
                        is_continuation: false,
                    }]
                })
                .unwrap_or_default());
        };

        let mut stmt = conn.prepare(&format!(
            "SELECT {}, li.position, \
                    EXISTS (SELECT 1 FROM continuations c WHERE c.fragment_id = f.id) \
             FROM line_index li JOIN fragments f ON f.id = li.fragment_id \
             WHERE li.root_id = ?1 ORDER BY li.position",
            FRAGMENT_COLUMNS
                .split(", ")
                .map(|c| format!("f.{c}"))
                .collect::<Vec<_>>()
                .join(", ")
        ))?;

        let moments: Vec<LineMoment> = stmt
            .query_map([&root], |r| {
                Ok(LineMoment {
                    fragment: map_fragment(r)?,
                    position: r.get(8)?,
                    is_continuation: r.get::<_, i64>(9)? != 0,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(moments)
    }

    /// Keep present. Bounded, with no category and no ordering.
    ///
    /// At the limit this refuses rather than releasing something to make room. Auto-releasing
    /// the oldest would be an ordering system, which the product explicitly rules out — and it
    /// would quietly drop something the person had decided to keep. Refusing puts that
    /// decision back where it belongs.
    ///
    /// Returns how many are held after the call, and whether this one was taken.
    pub fn hold_fragment(&self, id: &str) -> Result<(i64, bool), rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();

        let already: i64 = conn.query_row(
            "SELECT COUNT(*) FROM holds WHERE fragment_id = ?1 AND released_at IS NULL",
            [id],
            |r| r.get(0),
        )?;
        let held: i64 = conn.query_row(
            "SELECT COUNT(*) FROM holds h JOIN fragments f ON f.id = h.fragment_id \
             WHERE h.released_at IS NULL AND f.removed_at IS NULL",
            [],
            |r| r.get(0),
        )?;

        // Re-holding something already held is not a new hold, so it cannot hit the limit.
        if already == 0 && held >= MAX_HELD {
            return Ok((held, false));
        }

        conn.execute(
            "INSERT INTO holds (fragment_id, held_at, released_at) VALUES (?1, ?2, NULL) \
             ON CONFLICT(fragment_id) DO UPDATE SET held_at = ?2, released_at = NULL",
            rusqlite::params![id, now],
        )?;
        Ok((if already == 0 { held + 1 } else { held }, true))
    }

    /// Release lets material recede into the Record. It is not a delete.
    pub fn release_fragment(&self, id: &str) -> Result<(), rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE holds SET released_at = ?1 WHERE fragment_id = ?2 AND released_at IS NULL",
            rusqlite::params![now, id],
        )?;
        Ok(())
    }

    pub fn held_fragments(&self) -> Result<Vec<HeldFragment>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(&format!(
            "SELECT {}, h.held_at FROM holds h JOIN fragments f ON f.id = h.fragment_id \
             WHERE h.released_at IS NULL AND f.removed_at IS NULL ORDER BY h.held_at DESC",
            FRAGMENT_COLUMNS
                .split(", ")
                .map(|c| format!("f.{c}"))
                .collect::<Vec<_>>()
                .join(", ")
        ))?;
        let held: Vec<HeldFragment> = stmt
            .query_map([], |r| {
                Ok(HeldFragment {
                    fragment: map_fragment(r)?,
                    held_at: r.get(8)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(held)
    }
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeldFragment {
    pub fragment: Fragment,
    /// When it was put here. A Return can say how long something has been kept present.
    pub held_at: String,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindHit {
    pub fragment: Fragment,
    /// Which material answered: "body" (what the person wrote), "transcript" (derived from
    /// audio) or "source" (what a page was called). Find must be able to say which, so an
    /// exact phrase a person typed can outrank a word that only appears in a page title.
    pub matched_in: String,
}

impl Db {
    /// The immediate past, newest first. Backs Now.
    pub fn recent_fragments(&self, limit: i64) -> Result<Vec<Fragment>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(&format!(
            "SELECT {FRAGMENT_COLUMNS} FROM fragments \
             WHERE removed_at IS NULL ORDER BY captured_at DESC, id DESC LIMIT ?1"
        ))?;
        let out: Vec<Fragment> = stmt
            .query_map([limit], map_fragment)?
            .collect::<Result<_, _>>()?;
        Ok(out)
    }

    /// One page of the Record, walking backwards from `before`.
    ///
    /// Cursor-based rather than offset-based: at 50,000 fragments an OFFSET scan is the
    /// difference between instant and noticeable, and the Record is meant to be traversed.
    pub fn fragments_before(
        &self,
        before: Option<&str>,
        limit: i64,
    ) -> Result<Vec<Fragment>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let sql = format!(
            "SELECT {FRAGMENT_COLUMNS} FROM fragments \
             WHERE removed_at IS NULL AND (?1 IS NULL OR captured_at < ?1) \
             ORDER BY captured_at DESC, id DESC LIMIT ?2"
        );
        let mut stmt = conn.prepare(&sql)?;
        let out: Vec<Fragment> = stmt
            .query_map(rusqlite::params![before, limit], map_fragment)?
            .collect::<Result<_, _>>()?;
        Ok(out)
    }

    /// Fragments captured inside a half-open time range, oldest first.
    /// Backs standing in a month or a year.
    pub fn fragments_between(
        &self,
        from: &str,
        to: &str,
        limit: i64,
    ) -> Result<Vec<Fragment>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(&format!(
            "SELECT {FRAGMENT_COLUMNS} FROM fragments \
             WHERE removed_at IS NULL AND captured_at >= ?1 AND captured_at < ?2 \
             ORDER BY captured_at ASC, id ASC LIMIT ?3"
        ))?;
        let out: Vec<Fragment> = stmt
            .query_map(rusqlite::params![from, to, limit], map_fragment)?
            .collect::<Result<_, _>>()?;
        Ok(out)
    }

    /// How many fragments were captured inside a half-open range.
    ///
    /// The menu-bar menu says `today · n`, and it says it whether or not the window is
    /// open, so the count is a count rather than the length of a list the tray would
    /// otherwise have to load to throw away.
    pub fn count_between(&self, from: &str, to: &str) -> Result<i64, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        conn.query_row(
            "SELECT COUNT(*) FROM fragments \
             WHERE removed_at IS NULL AND captured_at >= ?1 AND captured_at < ?2",
            rusqlite::params![from, to],
            |r| r.get(0),
        )
    }

    /// Exact textual retrieval. This is Find's primary answer; anything by meaning is a
    /// separate, separately-labelled call, because a guess must never look like a match.
    pub fn find_fragments(&self, query: &str, limit: i64) -> Result<Vec<FindHit>, rusqlite::Error> {
        let match_query = fts_match_query(query);
        if match_query.is_empty() {
            return Ok(Vec::new());
        }
        let terms: Vec<String> = query
            .split_whitespace()
            .map(|t| t.to_lowercase())
            .collect();

        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(&format!(
            "SELECT {}, t.body, t.transcript, t.source \
             FROM fragments_fts t \
             JOIN fragments f ON f.id = t.fragment_id \
             WHERE fragments_fts MATCH ?1 AND f.removed_at IS NULL \
             ORDER BY f.captured_at DESC LIMIT ?2",
            FRAGMENT_COLUMNS
                .split(", ")
                .map(|c| format!("f.{c}"))
                .collect::<Vec<_>>()
                .join(", ")
        ))?;

        let out: Vec<FindHit> = stmt
            .query_map(rusqlite::params![match_query, limit], |r| {
                let fragment = map_fragment(r)?;
                let body: String = r.get(8)?;
                let transcript: String = r.get(9)?;
                let source: String = r.get(10)?;

                // Which material actually answered.
                //
                // Worked out from the text of each indexed column rather than from whether a
                // column happens to be non-empty — the earlier version asked the latter and so
                // reported "body" for a title match, which is Find claiming something untrue
                // about where a result came from.
                let hit_in = |text: &str| {
                    let lowered = text.to_lowercase();
                    !text.is_empty() && terms.iter().any(|t| lowered.contains(t))
                };
                let matched_in = if fragment.capture_method == "voice" && hit_in(&transcript) {
                    // For a voice fragment the words ARE the transcript: derived from audio,
                    // which is worth saying even though they also sit in the body.
                    "transcript"
                } else if hit_in(&body) {
                    "body"
                } else if hit_in(&transcript) {
                    "transcript"
                } else if hit_in(&source) {
                    "source"
                } else {
                    // FTS matched on a form the substring test cannot see (a stem, a
                    // diacritic fold). The body is the honest default, not a guess at a title.
                    "body"
                };

                Ok(FindHit {
                    fragment,
                    matched_in: matched_in.to_string(),
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(out)
    }

    /// The first and last capture times in the Record, or None when it is empty.
    /// Tells the Record which years exist without loading any of them.
    pub fn record_span(&self) -> Result<Option<(String, String)>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let span: Option<(Option<String>, Option<String>)> = conn
            .query_row(
                "SELECT MIN(captured_at), MAX(captured_at) FROM fragments WHERE removed_at IS NULL",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        Ok(match span {
            Some((Some(a), Some(b))) => Some((a, b)),
            _ => None,
        })
    }

    /// How much material each month of a year holds. Backs the year row: where material
    /// exists, not an analysis of what it means.
    pub fn month_density(&self, year: i32) -> Result<Vec<i64>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let mut counts = vec![0i64; 12];
        let mut stmt = conn.prepare(
            "SELECT CAST(strftime('%m', captured_at) AS INTEGER), COUNT(*) \
             FROM fragments WHERE removed_at IS NULL AND strftime('%Y', captured_at) = ?1 \
             GROUP BY 1",
        )?;
        let mut rows = stmt.query([year.to_string()])?;
        while let Some(r) = rows.next()? {
            let month: i64 = r.get(0)?;
            let n: i64 = r.get(1)?;
            if (1..=12).contains(&month) {
                counts[(month - 1) as usize] = n;
            }
        }
        Ok(counts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn db() -> Db {
        Db::open(PathBuf::from(":memory:")).unwrap()
    }

    fn body_at(db: &Db, id: &str) -> String {
        let conn = db.0.lock().unwrap();
        conn.query_row("SELECT body FROM fragments WHERE id = ?1", [id], |r| r.get(0))
            .unwrap()
    }

    fn captured_at(db: &Db, id: &str) -> String {
        let conn = db.0.lock().unwrap();
        conn.query_row("SELECT captured_at FROM fragments WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .unwrap()
    }

    fn find_ids(db: &Db, query: &str) -> Vec<String> {
        let conn = db.0.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT fragment_id FROM fragments_fts WHERE fragments_fts MATCH ?1")
            .unwrap();
        let ids: Vec<String> = stmt
            .query_map([super::fts_match_query(query)], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        ids
    }

    #[test]
    fn capture_takes_no_classification() {
        let db = db();
        let f = db.capture_fragment("dinner friday", "typed", Some("desktop")).unwrap();
        assert_eq!(f.body, "dinner friday");
        assert_eq!(f.capture_method, "typed");
        assert_eq!(f.correction_count, 0);
        assert!(!f.id.is_empty());
    }

    #[test]
    fn a_one_word_fragment_is_as_valid_as_a_paragraph() {
        let db = db();
        let short = db.capture_fragment("ok", "typed", None).unwrap();
        let long = db
            .capture_fragment(
                "Maybe the reason I keep abandoning tools is that they ask me to decide what a \
                 thing is before I've finished having it.",
                "typed",
                None,
            )
            .unwrap();
        // Same table, same columns, same status. Nothing promotes one over the other.
        assert_eq!(short.capture_method, long.capture_method);
        assert_eq!(short.correction_count, long.correction_count);
    }

    /// The central guarantee of the product.
    #[test]
    fn continue_leaves_the_earlier_moment_untouched() {
        let db = db();
        let first = db
            .capture_fragment("idea: a notebook that doesn't ask what the note is", "typed", None)
            .unwrap();
        let before_body = body_at(&db, &first.id);
        let before_time = captured_at(&db, &first.id);

        let second = db
            .continue_fragment(&first.id, "a tag is a folder that's embarrassed about it", "typed", None, false)
            .unwrap();

        assert_ne!(second.id, first.id, "Continue creates a new fragment");
        assert_eq!(body_at(&db, &first.id), before_body, "earlier wording unchanged");
        assert_eq!(captured_at(&db, &first.id), before_time, "earlier date unchanged");
        assert_eq!(
            db.fragment_history(&first.id).unwrap().len(),
            1,
            "continuing is not correcting: it adds no revision to the earlier moment"
        );
    }

    #[test]
    fn a_continuation_is_dated_now_not_backdated_to_the_original() {
        let db = db();
        let first = db.capture_fragment("original", "typed", None).unwrap();
        let second = db.continue_fragment(&first.id, "later", "typed", None, false).unwrap();
        assert!(
            second.captured_at >= first.captured_at,
            "a continuation carries its own capture time"
        );
    }

    #[test]
    fn a_line_reads_from_its_first_moment_forward() {
        let db = db();
        let a = db.capture_fragment("2021", "typed", None).unwrap();
        let b = db.continue_fragment(&a.id, "2023", "typed", None, false).unwrap();
        let c = db.continue_fragment(&b.id, "today", "typed", None, false).unwrap();

        let line = db.line_for(&b.id).unwrap();
        let bodies: Vec<&str> = line.iter().map(|m| m.fragment.body.as_str()).collect();
        assert_eq!(bodies, vec!["2021", "2023", "today"]);
        assert_eq!(line[0].position, 0);
        assert!(!line[0].is_continuation, "the first moment continues nothing");
        assert!(line[1].is_continuation);
        assert_eq!(line[2].fragment.id, c.id);

        // Opening any moment reveals the same line.
        assert_eq!(db.line_for(&a.id).unwrap().len(), 3);
        assert_eq!(db.line_for(&c.id).unwrap().len(), 3);
    }

    #[test]
    fn a_fragment_nothing_continued_is_its_own_whole_line() {
        let db = db();
        let f = db.capture_fragment("alone", "typed", None).unwrap();
        let line = db.line_for(&f.id).unwrap();
        assert_eq!(line.len(), 1);
        assert!(!line[0].is_continuation);
    }

    #[test]
    fn accepting_a_suggestion_links_without_re_dating_the_capture() {
        let db = db();
        let old = db.capture_fragment("pasta water too salty again. less.", "typed", None).unwrap();
        let new = db
            .capture_fragment("tell K the salty pasta was the pot, not me", "typed", None)
            .unwrap();
        let captured_before = captured_at(&db, &new.id);

        db.link_continuation(&new.id, &old.id).unwrap();

        assert_eq!(
            captured_at(&db, &new.id),
            captured_before,
            "the capture already happened; accepting a suggestion only adds a relationship"
        );
        let line = db.line_for(&new.id).unwrap();
        assert_eq!(line.len(), 2);
        assert_eq!(line[0].fragment.id, old.id);
    }

    #[test]
    fn declining_a_suggestion_leaves_a_perfectly_good_fragment() {
        let db = db();
        let _old = db.capture_fragment("earlier", "typed", None).unwrap();
        let new = db.capture_fragment("standalone", "typed", None).unwrap();
        // No link_continuation call at all.
        assert_eq!(db.line_for(&new.id).unwrap().len(), 1);
        assert_eq!(body_at(&db, &new.id), "standalone");
    }

    /// Correction keeps what it replaced and does not move the moment in time.
    #[test]
    fn correction_retains_the_earlier_wording_and_the_original_date() {
        let db = db();
        let f = db.capture_fragment("teh wording", "typed", None).unwrap();
        let original_time = captured_at(&db, &f.id);

        let count = db.correct_fragment(&f.id, "the wording").unwrap();

        assert_eq!(count, 1);
        assert_eq!(body_at(&db, &f.id), "the wording");
        assert_eq!(
            captured_at(&db, &f.id),
            original_time,
            "correcting wording must not re-date the moment"
        );

        let history = db.fragment_history(&f.id).unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].body, "teh wording", "the earlier wording survives");
        assert_eq!(history[1].body, "the wording", "history ends with the current wording");
    }

    #[test]
    fn repeated_corrections_stack_oldest_first() {
        let db = db();
        let f = db.capture_fragment("one", "typed", None).unwrap();
        db.correct_fragment(&f.id, "two").unwrap();
        db.correct_fragment(&f.id, "three").unwrap();
        let n = db.correct_fragment(&f.id, "four").unwrap();

        assert_eq!(n, 3);
        let bodies: Vec<String> = db
            .fragment_history(&f.id)
            .unwrap()
            .into_iter()
            .map(|r| r.body)
            .collect();
        assert_eq!(bodies, vec!["one", "two", "three", "four"]);
    }

    #[test]
    fn saving_unchanged_text_manufactures_no_history() {
        let db = db();
        let f = db.capture_fragment("unchanged", "typed", None).unwrap();
        db.correct_fragment(&f.id, "unchanged").unwrap();
        db.correct_fragment(&f.id, "unchanged").unwrap();

        assert_eq!(db.fragment_history(&f.id).unwrap().len(), 1);
        let conn = db.0.lock().unwrap();
        let (count, corrected): (i64, Option<String>) = conn
            .query_row(
                "SELECT correction_count, corrected_at FROM fragments WHERE id = ?1",
                [&f.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(count, 0);
        assert!(corrected.is_none(), "an unchanged save is not a correction");
    }

    #[test]
    fn correcting_a_continuation_does_not_disturb_the_rest_of_the_line() {
        let db = db();
        let a = db.capture_fragment("first", "typed", None).unwrap();
        let b = db.continue_fragment(&a.id, "secnod", "typed", None, false).unwrap();
        let c = db.continue_fragment(&b.id, "third", "typed", None, false).unwrap();

        db.correct_fragment(&b.id, "second").unwrap();

        assert_eq!(body_at(&db, &a.id), "first");
        assert_eq!(body_at(&db, &c.id), "third");
        assert_eq!(db.fragment_history(&a.id).unwrap().len(), 1);
        let line = db.line_for(&a.id).unwrap();
        assert_eq!(
            line.iter().map(|m| m.fragment.body.as_str()).collect::<Vec<_>>(),
            vec!["first", "second", "third"]
        );
    }

    #[test]
    fn hold_and_release_keep_material_present_then_let_it_recede() {
        let db = db();
        let f = db.capture_fragment("boiler guy — Tues between 12 and 3", "typed", None).unwrap();

        assert!(db.held_fragments().unwrap().is_empty());
        assert_eq!(db.hold_fragment(&f.id).unwrap(), (1, true));
        let held = db.held_fragments().unwrap();
        assert_eq!(held.len(), 1);
        assert_eq!(held[0].fragment.id, f.id);

        db.release_fragment(&f.id).unwrap();
        assert!(db.held_fragments().unwrap().is_empty());
        // Releasing is not deleting: the fragment is still in the Record.
        assert_eq!(body_at(&db, &f.id), "boiler guy — Tues between 12 and 3");
    }

    #[test]
    fn holding_twice_does_not_duplicate_or_error() {
        let db = db();
        let f = db.capture_fragment("keep", "typed", None).unwrap();
        db.hold_fragment(&f.id).unwrap();
        // Re-holding what is already held is not a second hold.
        assert_eq!(db.hold_fragment(&f.id).unwrap(), (1, true));
        assert_eq!(db.held_fragments().unwrap().len(), 1);
    }

    #[test]
    fn keeping_present_is_bounded_and_refuses_rather_than_dropping_something() {
        let db = db();
        let mut ids = Vec::new();
        for i in 0..MAX_HELD {
            let f = db.capture_fragment(&format!("keep {i}"), "typed", None).unwrap();
            assert_eq!(db.hold_fragment(&f.id).unwrap(), (i + 1, true));
            ids.push(f.id);
        }

        let one_too_many = db.capture_fragment("keep this too", "typed", None).unwrap();
        let (count, taken) = db.hold_fragment(&one_too_many.id).unwrap();
        assert!(!taken, "the bound refuses");
        assert_eq!(count, MAX_HELD);

        // Nothing was quietly released to make room.
        let held = db.held_fragments().unwrap();
        assert_eq!(held.len() as i64, MAX_HELD);
        for id in &ids {
            assert!(held.iter().any(|h| &h.fragment.id == id), "{id} was dropped");
        }

        // An explicit release makes room, and only then.
        db.release_fragment(&ids[0]).unwrap();
        assert_eq!(db.hold_fragment(&one_too_many.id).unwrap(), (MAX_HELD, true));
    }

    #[test]
    fn a_released_hold_does_not_count_against_the_bound() {
        let db = db();
        for i in 0..MAX_HELD {
            let f = db.capture_fragment(&format!("k{i}"), "typed", None).unwrap();
            db.hold_fragment(&f.id).unwrap();
            db.release_fragment(&f.id).unwrap();
        }
        let f = db.capture_fragment("fresh", "typed", None).unwrap();
        assert_eq!(db.hold_fragment(&f.id).unwrap(), (1, true));
    }

    #[test]
    fn re_holding_after_release_works() {
        let db = db();
        let f = db.capture_fragment("keep", "typed", None).unwrap();
        db.hold_fragment(&f.id).unwrap();
        db.release_fragment(&f.id).unwrap();
        db.hold_fragment(&f.id).unwrap();
        assert_eq!(db.held_fragments().unwrap().len(), 1);
    }

    #[test]
    fn find_matches_what_a_person_wrote() {
        let db = db();
        let f = db.capture_fragment("putting something in a folder", "typed", None).unwrap();
        assert_eq!(find_ids(&db, "folder"), vec![f.id]);
    }

    #[test]
    fn find_reaches_a_voice_transcript_and_a_source_title() {
        let db = db();
        let spoken = db.capture_fragment("", "voice", None).unwrap();
        let shared = db.capture_fragment("", "url", None).unwrap();
        {
            let conn = db.0.lock().unwrap();
            conn.execute(
                "INSERT INTO voice_captures (fragment_id, audio_path, duration_ms, recorded_at) \
                 VALUES (?1, '/tmp/a.wav', 11000, '2026-01-01T00:00:00Z')",
                [&spoken.id],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO voice_transcripts (fragment_id, machine_transcript, state) \
                 VALUES (?1, 'a tag is a folder that is embarrassed about it', 'ok')",
                [&spoken.id],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO encounters (fragment_id, url_raw, url_key, shared_at) \
                 VALUES (?1, 'https://theatlantic.com/x', 'theatlantic.com/x', '2026-01-01T00:00:00Z')",
                [&shared.id],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO encounter_enrichment (encounter_id, domain, title, state) \
                 VALUES (last_insert_rowid(), 'theatlantic.com', 'Why Everyone Suddenly Wants a Second Brain', 'ok')",
                [],
            )
            .unwrap();
            super::sync_fts(&conn, &spoken.id).unwrap();
            super::sync_fts(&conn, &shared.id).unwrap();
        }

        assert_eq!(find_ids(&db, "embarrassed"), vec![spoken.id]);
        assert_eq!(find_ids(&db, "Suddenly"), vec![shared.id.clone()]);
        assert_eq!(find_ids(&db, "theatlantic.com"), vec![shared.id]);
    }

    #[test]
    fn find_follows_a_correction() {
        let db = db();
        let f = db.capture_fragment("heating enginer", "typed", None).unwrap();
        db.correct_fragment(&f.id, "heating engineer").unwrap();

        assert!(find_ids(&db, "enginer").is_empty(), "the old typo stops matching");
        assert_eq!(find_ids(&db, "engineer"), vec![f.id]);
    }

    #[test]
    fn capture_survives_text_that_would_break_a_query() {
        let db = db();
        // Quotes, punctuation and a bare URL are ordinary material, not edge cases.
        let odd = "\"the map is not the territory\" — see https://example.com/a?b=1&c=2 (50% ok)";
        let f = db.capture_fragment(odd, "typed", None).unwrap();
        assert_eq!(body_at(&db, &f.id), odd);
        assert_eq!(find_ids(&db, "territory"), vec![f.id]);
    }
}

#[cfg(test)]
mod find_query_tests {
    use super::fts_match_query;

    #[test]
    fn plain_words_are_anded() {
        assert_eq!(fts_match_query("heating engineer"), "\"heating\" AND \"engineer\"");
    }

    #[test]
    fn punctuation_that_is_fts_syntax_is_neutralised() {
        // Each of these is a syntax error if passed through raw.
        for input in ["theatlantic.com", "second-brain", "note:", "a*b", "(x)", "^start"] {
            let q = fts_match_query(input);
            assert!(q.starts_with('"') && q.ends_with('"'), "{input} -> {q}");
        }
    }

    #[test]
    fn a_quote_in_the_query_cannot_break_out() {
        assert_eq!(fts_match_query("\"sort later\""), "\"\"\"sort\" AND \"later\"\"\"");
    }

    #[test]
    fn empty_input_yields_empty_query() {
        assert_eq!(fts_match_query("   "), "");
    }
}

#[cfg(test)]
mod read_tests {
    use super::*;
    use std::path::PathBuf;

    fn db() -> Db {
        Db::open(PathBuf::from(":memory:")).unwrap()
    }

    fn seed(db: &Db, rows: &[(&str, &str)]) {
        let conn = db.0.lock().unwrap();
        for (id, at) in rows {
            conn.execute(
                "INSERT INTO fragments (id, body, captured_at) VALUES (?1, ?1, ?2)",
                rusqlite::params![id, at],
            )
            .unwrap();
            super::sync_fts(&conn, id).unwrap();
        }
    }

    /// `today · n` in the menu-bar menu.
    ///
    /// It counts the same material the Record shows, which means removed fragments are not
    /// in it: a number in the menu bar that disagrees with the record below it is worse
    /// than no number.
    #[test]
    fn the_day_is_counted_without_loading_it() {
        let db = db();
        seed(
            &db,
            &[
                ("yesterday", "2026-09-20T22:00:00Z"),
                ("morning", "2026-09-21T08:30:00Z"),
                ("noon", "2026-09-21T12:00:00Z"),
                ("gone", "2026-09-21T13:00:00Z"),
                ("tomorrow", "2026-09-22T00:30:00Z"),
            ],
        );
        let (from, to) = ("2026-09-21T00:00:00Z", "2026-09-22T00:00:00Z");
        assert_eq!(db.count_between(from, to).unwrap(), 3);

        db.remove_fragment("gone").unwrap();
        assert_eq!(db.count_between(from, to).unwrap(), 2);
    }

    #[test]
    fn a_day_with_nothing_in_it_counts_zero() {
        let db = db();
        seed(&db, &[("only", "2026-09-20T22:00:00Z")]);
        assert_eq!(
            db.count_between("2026-09-21T00:00:00Z", "2026-09-22T00:00:00Z")
                .unwrap(),
            0
        );
    }

    #[test]
    fn the_record_pages_backwards_without_repeating_or_skipping() {
        let db = db();
        seed(
            &db,
            &[
                ("a", "2025-01-01T00:00:00Z"),
                ("b", "2025-02-01T00:00:00Z"),
                ("c", "2025-03-01T00:00:00Z"),
                ("d", "2025-04-01T00:00:00Z"),
                ("e", "2025-05-01T00:00:00Z"),
            ],
        );

        let page1 = db.fragments_before(None, 2).unwrap();
        assert_eq!(page1.iter().map(|f| f.id.as_str()).collect::<Vec<_>>(), vec!["e", "d"]);

        let cursor = page1.last().unwrap().captured_at.clone();
        let page2 = db.fragments_before(Some(&cursor), 2).unwrap();
        assert_eq!(page2.iter().map(|f| f.id.as_str()).collect::<Vec<_>>(), vec!["c", "b"]);

        let cursor2 = page2.last().unwrap().captured_at.clone();
        let page3 = db.fragments_before(Some(&cursor2), 2).unwrap();
        assert_eq!(page3.iter().map(|f| f.id.as_str()).collect::<Vec<_>>(), vec!["a"]);

        let cursor3 = page3.last().unwrap().captured_at.clone();
        assert!(db.fragments_before(Some(&cursor3), 2).unwrap().is_empty());
    }

    #[test]
    fn standing_in_a_month_reads_oldest_first() {
        let db = db();
        seed(
            &db,
            &[
                ("older", "2021-02-01T00:00:00Z"),
                ("m1", "2021-03-02T00:00:00Z"),
                ("m2", "2021-03-20T00:00:00Z"),
                ("newer", "2021-04-01T00:00:00Z"),
            ],
        );
        let in_march = db
            .fragments_between("2021-03-01T00:00:00Z", "2021-04-01T00:00:00Z", 100)
            .unwrap();
        assert_eq!(
            in_march.iter().map(|f| f.id.as_str()).collect::<Vec<_>>(),
            vec!["m1", "m2"]
        );
    }

    #[test]
    fn month_density_reports_where_material_exists() {
        let db = db();
        seed(
            &db,
            &[
                ("a", "2021-03-02T00:00:00Z"),
                ("b", "2021-03-20T00:00:00Z"),
                ("c", "2021-11-05T00:00:00Z"),
                ("d", "2022-01-05T00:00:00Z"),
            ],
        );
        let d = db.month_density(2021).unwrap();
        assert_eq!(d.len(), 12);
        assert_eq!(d[2], 2, "march");
        assert_eq!(d[10], 1, "november");
        assert_eq!(d[0], 0, "january 2021 is empty, and says so");
        assert_eq!(db.month_density(2022).unwrap()[0], 1);
    }

    #[test]
    fn a_year_with_no_material_is_all_zeroes_not_an_error() {
        let db = db();
        assert_eq!(db.month_density(1999).unwrap(), vec![0; 12]);
    }

    #[test]
    fn find_says_which_material_answered() {
        let db = db();
        let typed = db.capture_fragment("a folder is a decision", "typed", None).unwrap();
        let shared = db.capture_fragment("", "url", None).unwrap();
        {
            let conn = db.0.lock().unwrap();
            conn.execute(
                "INSERT INTO encounters (fragment_id, url_raw, url_key, shared_at) \
                 VALUES (?1, 'https://x.test', 'x.test', '2026-01-01T00:00:00Z')",
                [&shared.id],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO encounter_enrichment (encounter_id, title, state) \
                 VALUES (last_insert_rowid(), 'On the folder problem', 'ok')",
                [],
            )
            .unwrap();
            super::sync_fts(&conn, &shared.id).unwrap();
        }

        let hits = db.find_fragments("folder", 10).unwrap();
        assert_eq!(hits.len(), 2);
        let typed_hit = hits.iter().find(|h| h.fragment.id == typed.id).unwrap();
        let shared_hit = hits.iter().find(|h| h.fragment.id == shared.id).unwrap();
        assert_eq!(typed_hit.matched_in, "body");
        assert_eq!(shared_hit.matched_in, "source");
    }

    #[test]
    fn find_with_nothing_typed_returns_nothing_rather_than_everything() {
        let db = db();
        seed(&db, &[("a", "2025-01-01T00:00:00Z")]);
        assert!(db.find_fragments("", 10).unwrap().is_empty());
        assert!(db.find_fragments("   ", 10).unwrap().is_empty());
    }

    #[test]
    fn removed_material_leaves_find_but_stays_in_the_record() {
        let db = db();
        let f = db.capture_fragment("a passing thought", "typed", None).unwrap();
        {
            let conn = db.0.lock().unwrap();
            conn.execute(
                "UPDATE fragments SET removed_at = '2025-01-01T00:00:00Z' WHERE id = ?1",
                [&f.id],
            )
            .unwrap();
            super::sync_fts(&conn, &f.id).unwrap();
        }
        assert!(db.find_fragments("passing", 10).unwrap().is_empty());

        let conn = db.0.lock().unwrap();
        let body: String = conn
            .query_row("SELECT body FROM fragments WHERE id = ?1", [&f.id], |r| r.get(0))
            .unwrap();
        assert_eq!(body, "a passing thought", "removal is not destruction");
    }
}
