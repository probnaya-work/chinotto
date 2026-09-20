//! Retrieval by meaning — the guesses.
//!
//! Kept in its own module, behind its own command, returning its own type, because the one
//! thing Find must never do is let a guess look like a match. Exact retrieval does not call
//! anything here and never waits on it: if the model is missing, slow, or still downloading,
//! Find still answers from the words.
//!
//! Everything this module produces is inferred, and every caller has to say so.

use super::record::Fragment;
use super::Db;
use rusqlite::OptionalExtension;

const MODEL: &str = "AllMiniLML6V2";
/// Below this, "close in meaning" is not a claim worth making.
const MIN_SIMILARITY: f32 = 0.45;

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Guess {
    pub fragment: Fragment,
    /// Cosine similarity. Shown to no one: it is a tie-breaker, never a stated reason.
    pub score: f32,
}

fn body_hash(body: &str) -> String {
    // Cheap content fingerprint: enough to notice a correction, not a security boundary.
    let mut h: u64 = 1469598103934665603;
    for b in body.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    format!("{h:x}")
}

fn to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn from_blob(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

impl Db {
    /// Embeds up to `limit` fragments that have no current vector.
    ///
    /// Incremental on purpose. Embedding a 50,000 fragment record in one pass would block
    /// the app on first launch after an upgrade; doing a slice at a time means Find by
    /// meaning simply gets better over the first few minutes instead of holding everything up.
    pub fn embed_pending(&self, limit: i64) -> Result<usize, String> {
        let pending: Vec<(String, String)> = {
            let conn = self.0.lock().map_err(|e| e.to_string())?;
            let mut stmt = conn
                .prepare(
                    // No vector, a vector from a different model, or one imported from v1
                    // with no recorded source wording. A correction deletes the row
                    // outright (see invalidate_embedding), so it lands in the first case.
                    "SELECT f.id, f.body FROM fragments f \
                     LEFT JOIN fragment_embeddings e ON e.fragment_id = f.id \
                     WHERE f.removed_at IS NULL \
                       AND (e.fragment_id IS NULL OR e.model <> ?1 OR e.body_hash = '') \
                     ORDER BY f.captured_at DESC \
                     LIMIT ?2",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(rusqlite::params![MODEL, limit], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(|e| e.to_string())?;
            rows.collect::<Result<_, _>>().map_err(|e| e.to_string())?
        };

        let mut done = 0;
        for (id, body) in pending {
            if body.trim().is_empty() {
                continue;
            }
            // One at a time so a single failure cannot lose the batch.
            let Ok(vector) = crate::embeddings::embed_text(&body) else {
                continue;
            };
            let conn = self.0.lock().map_err(|e| e.to_string())?;
            conn.execute(
                "INSERT INTO fragment_embeddings (fragment_id, embedding, model, computed_at, body_hash) \
                 VALUES (?1, ?2, ?3, ?4, ?5) \
                 ON CONFLICT(fragment_id) DO UPDATE SET \
                   embedding = excluded.embedding, model = excluded.model, \
                   computed_at = excluded.computed_at, body_hash = excluded.body_hash",
                rusqlite::params![
                    id,
                    to_blob(&vector),
                    MODEL,
                    chrono::Utc::now().to_rfc3339(),
                    body_hash(&body)
                ],
            )
            .map_err(|e| e.to_string())?;
            done += 1;
        }
        Ok(done)
    }

    /// Fragments close in meaning to `query`, excluding anything the person has rejected.
    ///
    /// `exclude` is the set of ids the words already answered, so a guess never repeats a
    /// match — the two lists are disjoint by construction, not by presentation.
    pub fn find_by_meaning(
        &self,
        query: &str,
        exclude: &[String],
        limit: usize,
    ) -> Result<Vec<Guess>, String> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let probe = crate::embeddings::embed_text(query)?;

        let conn = self.0.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT f.id, f.body, f.captured_at, f.capture_method, f.capture_origin, \
                        f.corrected_at, f.correction_count, f.legacy_edit_count, e.embedding \
                 FROM fragment_embeddings e JOIN fragments f ON f.id = e.fragment_id \
                 WHERE f.removed_at IS NULL \
                   AND NOT EXISTS ( \
                     SELECT 1 FROM trace_judgements j \
                     WHERE j.related_id = f.id AND j.judgement = 'rejected' AND j.kind = 'inferred')",
            )
            .map_err(|e| e.to_string())?;

        let mut hits: Vec<Guess> = stmt
            .query_map([], |r| {
                let blob: Vec<u8> = r.get(8)?;
                Ok(Guess {
                    fragment: Fragment {
                        id: r.get(0)?,
                        body: r.get(1)?,
                        captured_at: r.get(2)?,
                        capture_method: r.get(3)?,
                        capture_origin: r.get(4)?,
                        corrected_at: r.get(5)?,
                        correction_count: r.get(6)?,
                        legacy_edit_count: r.get(7)?,
                    },
                    score: crate::embeddings::cosine_similarity(&probe, &from_blob(&blob)),
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        hits.retain(|g| g.score >= MIN_SIMILARITY && !exclude.contains(&g.fragment.id));
        hits.sort_by(|a, b| b.score.total_cmp(&a.score));
        hits.truncate(limit);
        Ok(hits)
    }

    /// Records "not this". A rejection is a judgement a person made, so it is canonical and
    /// outlives every recomputation of the vectors.
    pub fn reject_guess(&self, query_fragment_id: &str, related_id: &str) -> Result<(), rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO trace_judgements (fragment_id, related_id, kind, judgement, judged_at) \
             VALUES (?1, ?2, 'inferred', 'rejected', ?3)",
            rusqlite::params![query_fragment_id, related_id, chrono::Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    /// How many fragments still have no current vector. Lets a caller decide whether it is
    /// worth offering "also by meaning" yet.
    pub fn pending_embedding_count(&self) -> Result<i64, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        conn.query_row(
            "SELECT COUNT(*) FROM fragments f \
             LEFT JOIN fragment_embeddings e ON e.fragment_id = f.id \
             WHERE f.removed_at IS NULL AND e.fragment_id IS NULL",
            [],
            |r| r.get(0),
        )
    }

    /// True when a fragment's stored vector matches its current wording.
    ///
    /// Nothing in the running app calls this — correction invalidates inline and the
    /// background pass re-embeds whatever is missing. It stays because it states the cache's
    /// freshness contract in one place, and the tests assert against it.
    #[allow(dead_code)]
    pub fn embedding_is_current(&self, id: &str) -> Result<bool, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let row: Option<(String, String)> = conn
            .query_row(
                "SELECT e.body_hash, f.body FROM fragment_embeddings e \
                 JOIN fragments f ON f.id = e.fragment_id WHERE e.fragment_id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        Ok(match row {
            Some((hash, body)) => !hash.is_empty() && hash == body_hash(&body),
            None => false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn db() -> Db {
        Db::open(PathBuf::from(":memory:")).unwrap()
    }

    fn put_vector(db: &Db, id: &str, body: &str) {
        let conn = db.0.lock().unwrap();
        conn.execute(
            "INSERT INTO fragment_embeddings (fragment_id, embedding, model, computed_at, body_hash) \
             VALUES (?1, X'0000803F', ?2, '2026-01-01T00:00:00Z', ?3)",
            rusqlite::params![id, MODEL, super::body_hash(body)],
        )
        .unwrap();
    }

    #[test]
    fn a_vector_round_trips_through_the_blob() {
        let v = vec![0.5_f32, -0.25, 1.0, 0.0];
        assert_eq!(from_blob(&to_blob(&v)), v);
    }

    #[test]
    fn an_empty_query_asks_nothing_of_the_model() {
        let db = db();
        // Must not reach embed_text at all — no model, no network, still fine.
        assert!(db.find_by_meaning("", &[], 5).unwrap().is_empty());
        assert!(db.find_by_meaning("   ", &[], 5).unwrap().is_empty());
    }

    #[test]
    fn correcting_a_fragment_discards_the_vector_that_described_the_old_wording() {
        let db = db();
        let f = db.capture_fragment("heating enginer", "typed", None).unwrap();
        put_vector(&db, &f.id, "heating enginer");
        assert!(db.embedding_is_current(&f.id).unwrap());

        db.correct_fragment(&f.id, "heating engineer").unwrap();

        assert!(
            !db.embedding_is_current(&f.id).unwrap(),
            "a vector describing wording that no longer exists must not survive"
        );
        let conn = db.0.lock().unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM fragment_embeddings WHERE fragment_id = ?1", [&f.id], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn a_vector_imported_from_v1_counts_as_stale() {
        let db = db();
        let f = db.capture_fragment("something", "typed", None).unwrap();
        {
            let conn = db.0.lock().unwrap();
            conn.execute(
                "INSERT INTO fragment_embeddings (fragment_id, embedding, model, computed_at, body_hash) \
                 VALUES (?1, X'0000803F', ?2, '', '')",
                rusqlite::params![f.id, MODEL],
            )
            .unwrap();
        }
        // No recorded source wording means we cannot claim it describes the current text.
        assert!(!db.embedding_is_current(&f.id).unwrap());
    }

    #[test]
    fn rejecting_a_guess_is_a_judgement_that_outlives_the_cache() {
        let db = db();
        let a = db.capture_fragment("boiler guy — Tues between 12 and 3", "typed", None).unwrap();
        let b = db.capture_fragment("heating engineer", "typed", None).unwrap();

        db.reject_guess(&b.id, &a.id).unwrap();

        let conn = db.0.lock().unwrap();
        // Rejections live in canonical material, not in the derived traces cache.
        let judgement: String = conn
            .query_row(
                "SELECT judgement FROM trace_judgements WHERE fragment_id = ?1 AND related_id = ?2",
                rusqlite::params![b.id, a.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(judgement, "rejected");

        // Blowing away every vector changes nothing about what the person decided.
        conn.execute("DELETE FROM fragment_embeddings", []).unwrap();
        let still: i64 = conn
            .query_row("SELECT COUNT(*) FROM trace_judgements", [], |r| r.get(0))
            .unwrap();
        assert_eq!(still, 1);
    }

    #[test]
    fn rejecting_the_same_guess_twice_does_not_duplicate_it() {
        let db = db();
        let a = db.capture_fragment("one", "typed", None).unwrap();
        let b = db.capture_fragment("two", "typed", None).unwrap();
        db.reject_guess(&b.id, &a.id).unwrap();
        db.reject_guess(&b.id, &a.id).unwrap();

        let conn = db.0.lock().unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM trace_judgements", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn pending_count_tracks_what_still_needs_embedding() {
        let db = db();
        let a = db.capture_fragment("one", "typed", None).unwrap();
        let _b = db.capture_fragment("two", "typed", None).unwrap();
        assert_eq!(db.pending_embedding_count().unwrap(), 2);

        put_vector(&db, &a.id, "one");
        assert_eq!(db.pending_embedding_count().unwrap(), 1);
    }
}
