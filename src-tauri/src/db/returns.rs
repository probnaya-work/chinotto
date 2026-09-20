//! Return: older material re-entering the edge, with the evidence for why.
//!
//! The rule this module exists to enforce is that a Return must be able to say why it came
//! back, in the person's own material. Every candidate below produces its evidence in the
//! same step that produces the candidate — there is no path that surfaces something and
//! then looks for a justification, and no scoring signal is ever the stated reason.
//!
//! **A date alone is context, not a reason.** Each surviving trigger names a concrete act:
//! something written, a source opened again, a line added to. It carries the fragment that
//! act produced, so the interface can quote it — `because at 11:05 today you wrote "…"` —
//! and can highlight the words the two actually share. A trigger that cannot do that does
//! not exist here:
//!
//!   * `interval` is gone. "From this day last year" is an anniversary, not a reason; the
//!     calendar did it, not the person.
//!   * `kept_present` is gone. Held material is already on the surface *because the person
//!     put it there*. Returning it would be the product telling someone about a decision
//!     they can see they made.
//!
//! Silence is valid. `select_return` returning None is the normal case, not a failure.

use super::Db;
use rusqlite::{Connection, OptionalExtension};

/// Hours after a Return before another may appear. Returns are sparse by construction.
const RETURN_COOLDOWN_HOURS: i64 = 20;
/// Days before the same fragment may return again.
const SAME_FRAGMENT_COOLDOWN_DAYS: i64 = 30;
/// Nothing younger than this is old enough to "return" — it is simply recent.
const MIN_RETURN_AGE_DAYS: i64 = 30;

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReturnEvidence {
    pub kind: String,
    /// The actual words, url or date. Never a score.
    pub detail: String,
    pub occurred_at: Option<String>,
    /// The fragment that caused this Return — the thing written, opened, or added.
    pub related_id: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Return {
    pub id: i64,
    pub fragment: super::record::Fragment,
    /// One of the closed set of reasons. The interface phrases it; it is not free text.
    pub reason: String,
    pub evidence: Vec<ReturnEvidence>,
    /// The causing fragment, so the surface can quote it. None only if it has since gone.
    pub because: Option<super::record::Fragment>,
}

/// Words too common to make a recurrence meaningful as an FTS probe.
const STOPWORDS: &[&str] = &[
    "about", "after", "again", "always", "another", "because", "before", "being", "between",
    "could", "every", "first", "going", "might", "never", "other", "really", "should", "since",
    "still", "their", "there", "these", "thing", "think", "those", "through", "under", "where",
    "which", "while", "would", "thought", "something", "anything",
];

/// The prototype's stoplist — used by the shared-run matcher, not by FTS.
const RUN_STOP: &[&str] = &[
    "the", "a", "an", "and", "or", "but", "of", "to", "in", "on", "at", "for", "with", "is",
    "are", "was", "were", "be", "been", "it", "its", "this", "that", "these", "those", "i",
    "me", "my", "we", "you", "your", "he", "she", "they", "them", "his", "her", "our", "their",
    "as", "by", "from", "not", "no", "so", "if", "then", "than", "too", "very", "just", "about",
    "into", "over", "under", "again", "there", "here", "what", "which", "who", "whom", "when",
    "where", "why", "how", "do", "does", "did", "done", "have", "has", "had", "having", "um",
    "uh", "ok", "okay", "i'm", "i've", "it's", "that's", "don't",
];

struct Token {
    w: String,
    s: String,
}

struct SharedRun {
    length: usize,
    a: String,
    b: String,
}

fn stem(word: &str) -> String {
    let mut s = word.replace('’', "'");
    if let Some(stripped) = s.strip_suffix("'s") {
        s = stripped.to_string();
    }
    if s.chars().count() > 5 {
        if let Some(stripped) = s.strip_suffix("ing") {
            s = stripped.to_string();
        }
    }
    for suf in ["edly", "ed", "es", "ly"] {
        if let Some(stripped) = s.strip_suffix(suf) {
            s = stripped.to_string();
            break;
        }
    }
    if s.chars().count() > 3 {
        if let Some(stripped) = s.strip_suffix('s') {
            s = stripped.to_string();
        }
    }
    if s.chars().count() > 4 {
        if let Some(stripped) = s.strip_suffix('e') {
            s = stripped.to_string();
        }
    }
    s
}

fn tokenize(text: &str) -> Vec<Token> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '\'' && c != '’')
        .filter(|w| !w.is_empty())
        .take(160)
        .map(|w| Token {
            w: w.to_string(),
            s: stem(w),
        })
        .collect()
}

/// Longest run of consecutive shared stems. Ported from the prototype's `sharedRun`.
fn shared_run(ta: &[Token], tb: &[Token]) -> Option<SharedRun> {
    let m = tb.len();
    let mut best: Option<SharedRun> = None;
    let mut prev = vec![0usize; m + 1];
    for i in 1..=ta.len() {
        let mut cur = vec![0usize; m + 1];
        for j in 1..=m {
            if ta[i - 1].s == tb[j - 1].s && ta[i - 1].s.chars().count() > 1 {
                cur[j] = prev[j - 1] + 1;
                if cur[j] >= 2 && best.as_ref().map(|b| b.length).unwrap_or(0) < cur[j] {
                    let a = &ta[i - cur[j]..i];
                    let carrying = a
                        .iter()
                        .filter(|t| !RUN_STOP.contains(&t.w.as_str()) && t.w.chars().count() > 2)
                        .count();
                    if carrying >= 2 || cur[j] >= 4 {
                        best = Some(SharedRun {
                            length: cur[j],
                            a: a.iter().map(|t| t.w.as_str()).collect::<Vec<_>>().join(" "),
                            b: tb[j - cur[j]..j]
                                .iter()
                                .map(|t| t.w.as_str())
                                .collect::<Vec<_>>()
                                .join(" "),
                        });
                    }
                }
            }
        }
        prev = cur;
    }
    best
}

/// Terms distinctive enough to be worth probing FTS with.
fn distinctive_terms(body: &str) -> Vec<String> {
    let mut terms: Vec<String> = body
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .map(|t| t.trim_matches('\'').to_lowercase())
        .filter(|t| t.chars().count() >= 5 && !STOPWORDS.contains(&t.as_str()))
        .collect();
    terms.sort();
    terms.dedup();
    terms
}

fn days_between(a: &str, b: &str) -> i64 {
    match (
        chrono::DateTime::parse_from_rfc3339(a),
        chrono::DateTime::parse_from_rfc3339(b),
    ) {
        (Ok(x), Ok(y)) => (y - x).num_days().abs(),
        _ => 0,
    }
}

fn hours_between(a: &str, b: &str) -> i64 {
    match (
        chrono::DateTime::parse_from_rfc3339(a),
        chrono::DateTime::parse_from_rfc3339(b),
    ) {
        (Ok(x), Ok(y)) => (y - x).num_hours().abs(),
        _ => i64::MAX,
    }
}

fn read_fragment(conn: &Connection, id: &str) -> Result<super::record::Fragment, rusqlite::Error> {
    conn.query_row(
        "SELECT id, body, captured_at, capture_method, capture_origin, corrected_at, \
         correction_count, legacy_edit_count FROM fragments WHERE id = ?1",
        [id],
        |r| {
            Ok(super::record::Fragment {
                id: r.get(0)?,
                body: r.get(1)?,
                captured_at: r.get(2)?,
                capture_method: r.get(3)?,
                capture_origin: r.get(4)?,
                corrected_at: r.get(5)?,
                correction_count: r.get(6)?,
                legacy_edit_count: r.get(7)?,
            })
        },
    )
}

fn load_return(conn: &Connection, return_id: i64) -> Result<Return, rusqlite::Error> {
    let (fragment_id, reason): (String, String) = conn.query_row(
        "SELECT fragment_id, reason FROM returns WHERE id = ?1",
        [return_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let mut stmt = conn.prepare(
        "SELECT kind, detail, occurred_at, related_id FROM return_evidence WHERE return_id = ?1",
    )?;
    let evidence: Vec<ReturnEvidence> = stmt
        .query_map([return_id], |r| {
            Ok(ReturnEvidence {
                kind: r.get(0)?,
                detail: r.get(1)?,
                occurred_at: r.get(2)?,
                related_id: r.get(3)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    let because_id = evidence.iter().find_map(|e| e.related_id.clone());
    let because = match because_id {
        Some(id) => read_fragment(conn, &id).ok(),
        None => None,
    };
    Ok(Return {
        id: return_id,
        fragment: read_fragment(conn, &fragment_id)?,
        reason,
        evidence,
        because,
    })
}

fn domain_of(url: &str) -> Option<String> {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let host = rest.split('/').next().unwrap_or(rest).trim();
    let host = host.trim_start_matches("www.");
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

impl Db {
    /// Picks at most one Return, or None. None is the ordinary answer.
    ///
    /// If a Return is already showing (no outcome yet), this returns that one rather than
    /// inventing another. `now` is passed in so the behaviour is testable.
    pub fn select_return(&self, now: &str) -> Result<Option<Return>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();

        if let Some(open_id) = conn
            .query_row(
                "SELECT id FROM returns WHERE outcome IS NULL ORDER BY surfaced_at DESC LIMIT 1",
                [],
                |r| r.get::<_, i64>(0),
            )
            .optional()?
        {
            return Ok(Some(load_return(&conn, open_id)?));
        }

        // Sparse: a gap after the last one, including ones already let go.
        if let Some(last) = conn
            .query_row(
                "SELECT surfaced_at FROM returns ORDER BY surfaced_at DESC LIMIT 1",
                [],
                |r| r.get::<_, String>(0),
            )
            .optional()?
        {
            if hours_between(&last, now) < RETURN_COOLDOWN_HOURS {
                return Ok(None);
            }
        }

        // Ordered by how directly the reason is grounded in something the person just did:
        // words they wrote, then a source they opened, then a line they built and left.
        let candidate = repeated_language(&conn, now)?
            .or(same_source(&conn, now)?)
            .or(line_continued(&conn, now)?);

        let Some((fragment_id, reason, evidence)) = candidate else {
            return Ok(None);
        };

        // The same material does not come back twice in a month.
        let recent_same: Option<String> = conn
            .query_row(
                "SELECT surfaced_at FROM returns WHERE fragment_id = ?1 ORDER BY surfaced_at DESC LIMIT 1",
                [&fragment_id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(prev) = recent_same {
            if days_between(&prev, now) < SAME_FRAGMENT_COOLDOWN_DAYS {
                return Ok(None);
            }
        }

        conn.execute(
            "INSERT INTO returns (fragment_id, surfaced_at, reason) VALUES (?1, ?2, ?3)",
            rusqlite::params![fragment_id, now, reason],
        )?;
        let return_id = conn.last_insert_rowid();

        for e in &evidence {
            conn.execute(
                "INSERT INTO return_evidence (return_id, kind, detail, occurred_at, related_id) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![return_id, e.kind, e.detail, e.occurred_at, e.related_id],
            )?;
        }

        load_return(&conn, return_id).map(Some)
    }

    /// Records what happened to a Return. Dismissing is an outcome, not a deletion.
    pub fn record_return_outcome(&self, id: i64, outcome: &str) -> Result<(), rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE returns SET outcome = ?1, outcome_at = ?2 WHERE id = ?3",
            rusqlite::params![outcome, now, id],
        )?;
        Ok(())
    }
}

type Candidate = Option<(String, String, Vec<ReturnEvidence>)>;

/// "You used this phrase again today." The strongest trigger: it is caused by something
/// the person just wrote, not by the passage of time. The evidence is a shared *run*,
/// not a single distinctive term, so the because-sentence can quote real words.
fn repeated_language(conn: &Connection, now: &str) -> Result<Candidate, rusqlite::Error> {
    let newest: Option<(String, String, String)> = conn
        .query_row(
            "SELECT id, body, captured_at FROM fragments WHERE removed_at IS NULL \
             ORDER BY captured_at DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((newest_id, body, newest_at)) = newest else {
        return Ok(None);
    };
    if hours_between(&newest_at, now) > 24 {
        return Ok(None);
    }

    let newest_tokens = tokenize(&body);
    let mut best: Option<(String, SharedRun)> = None;

    for term in distinctive_terms(&body) {
        let quoted = format!("\"{}\"", term.replace('"', "\"\""));
        let hits: i64 = conn.query_row(
            "SELECT COUNT(*) FROM fragments_fts WHERE fragments_fts MATCH ?1",
            [&quoted],
            |r| r.get(0),
        )?;
        if !(2..=6).contains(&hits) {
            continue;
        }

        let mut stmt = conn.prepare(
            "SELECT f.id, f.body, f.captured_at FROM fragments_fts t \
             JOIN fragments f ON f.id = t.fragment_id \
             WHERE fragments_fts MATCH ?1 AND f.id <> ?2 AND f.removed_at IS NULL \
             ORDER BY f.captured_at ASC LIMIT 12",
        )?;
        let rows = stmt.query_map(rusqlite::params![&quoted, &newest_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
        })?;
        for row in rows {
            let (old_id, old_body, old_at) = row?;
            if days_between(&old_at, now) < MIN_RETURN_AGE_DAYS {
                continue;
            }
            if let Some(run) = shared_run(&tokenize(&old_body), &newest_tokens) {
                if best.as_ref().map(|(_, r)| r.length).unwrap_or(0) < run.length {
                    best = Some((old_id, run));
                }
            }
        }
    }

    let Some((old_id, run)) = best else {
        return Ok(None);
    };
    Ok(Some((
        old_id,
        "repeated_language".to_string(),
        vec![
            ReturnEvidence {
                kind: "shared_phrase".into(),
                detail: run.a,
                occurred_at: None,
                related_id: None,
            },
            ReturnEvidence {
                kind: "shared_phrase".into(),
                detail: run.b,
                occurred_at: Some(newest_at),
                related_id: Some(newest_id),
            },
        ],
    )))
}

/// "You encountered this URL again", with both dates.
///
/// Reads `encounters.url_key`, which was written at capture. Nothing is matched by
/// similarity here — two encounters either share a stored key or they do not.
fn same_source(conn: &Connection, now: &str) -> Result<Candidate, rusqlite::Error> {
    let newest: Option<(String, String, String, String)> = conn
        .query_row(
            "SELECT f.id, f.captured_at, e.url_key, e.url_raw FROM fragments f \
             JOIN encounters e ON e.fragment_id = f.id \
             WHERE f.removed_at IS NULL ORDER BY f.captured_at DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;

    let Some((newest_id, newest_at, key, url_raw)) = newest else {
        return Ok(None);
    };
    if hours_between(&newest_at, now) > 24 {
        return Ok(None);
    }
    let Some(domain) = domain_of(&url_raw) else {
        return Ok(None);
    };

    let earlier: Option<(String, String)> = conn
        .query_row(
            "SELECT e.fragment_id, e.shared_at FROM encounters e \
             JOIN fragments f ON f.id = e.fragment_id \
             WHERE e.url_key = ?1 AND e.fragment_id <> ?2 AND f.removed_at IS NULL \
             ORDER BY e.shared_at ASC LIMIT 1",
            rusqlite::params![key, newest_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;

    let Some((old_id, old_at)) = earlier else {
        return Ok(None);
    };
    if days_between(&old_at, now) < MIN_RETURN_AGE_DAYS {
        return Ok(None);
    }

    Ok(Some((
        old_id,
        "same_source".to_string(),
        vec![ReturnEvidence {
            kind: "url".into(),
            detail: domain,
            occurred_at: Some(newest_at),
            related_id: Some(newest_id),
        }],
    )))
}

/// A line that was added to, then went quiet. The last addition is the causing act,
/// so the because-sentence can quote it rather than citing the calendar.
fn line_continued(conn: &Connection, now: &str) -> Result<Candidate, rusqlite::Error> {
    let row: Option<(String, String)> = conn
        .query_row(
            "SELECT li.root_id, MAX(f.captured_at) \
             FROM line_index li JOIN fragments f ON f.id = li.fragment_id \
             WHERE f.removed_at IS NULL \
             GROUP BY li.root_id HAVING li.line_length >= 3 \
             ORDER BY MAX(f.captured_at) ASC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;

    let Some((root_id, last_at)) = row else {
        return Ok(None);
    };
    if days_between(&last_at, now) < 60 {
        return Ok(None);
    }

    let last: Option<(String, String)> = conn
        .query_row(
            "SELECT f.id, f.body FROM line_index li JOIN fragments f ON f.id = li.fragment_id \
             WHERE li.root_id = ?1 AND f.removed_at IS NULL \
             ORDER BY li.position DESC LIMIT 1",
            [&root_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((last_id, last_body)) = last else {
        return Ok(None);
    };
    let snippet = last_body.split_whitespace().take(12).collect::<Vec<_>>().join(" ");
    if snippet.trim().is_empty() {
        return Ok(None);
    }

    Ok(Some((
        root_id,
        "line_continued".to_string(),
        vec![ReturnEvidence {
            kind: "continuation".into(),
            detail: snippet,
            occurred_at: Some(last_at),
            related_id: Some(last_id),
        }],
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn db() -> Db {
        Db::open(PathBuf::from(":memory:")).unwrap()
    }

    /// Inserts directly so capture times can be placed in the past.
    fn put(db: &Db, id: &str, body: &str, captured_at: &str) {
        let conn = db.0.lock().unwrap();
        conn.execute(
            "INSERT INTO fragments (id, body, captured_at, capture_method) VALUES (?1, ?2, ?3, 'typed')",
            rusqlite::params![id, body, captured_at],
        )
        .unwrap();
        super::super::record::sync_fts(&conn, id).unwrap();
    }

    fn set_captured(db: &Db, id: &str, captured_at: &str) {
        let conn = db.0.lock().unwrap();
        conn.execute(
            "UPDATE fragments SET captured_at = ?1 WHERE id = ?2",
            rusqlite::params![captured_at, id],
        )
        .unwrap();
    }

    const NOW: &str = "2026-09-19T17:00:00+00:00";

    #[test]
    fn an_empty_record_returns_nothing() {
        let db = db();
        assert!(db.select_return(NOW).unwrap().is_none());
    }

    #[test]
    fn silence_is_valid_when_nothing_recurs() {
        let db = db();
        put(&db, "a", "pharmacy before six", "2026-09-19T16:00:00+00:00");
        put(&db, "b", "tomatoes bread olive oil", "2025-01-04T10:00:00+00:00");
        assert!(db.select_return(NOW).unwrap().is_none());
    }

    #[test]
    fn recent_material_alone_is_not_a_return() {
        let db = db();
        put(&db, "a", "premature judgment again", "2026-09-19T16:00:00+00:00");
        put(&db, "b", "premature judgment", "2026-09-15T10:00:00+00:00");
        assert!(db.select_return(NOW).unwrap().is_none());
    }

    #[test]
    fn a_phrase_used_again_today_brings_back_the_older_moment_with_its_words() {
        let db = db();
        put(&db, "old", "filing is a kind of premature judgment", "2023-03-14T23:41:00+00:00");
        put(&db, "new", "still doing premature judgment on every note", "2026-09-19T16:30:00+00:00");

        let r = db.select_return(NOW).unwrap().expect("a recurrence should return");
        assert_eq!(r.fragment.id, "old", "the OLDER moment is what comes back");
        assert_eq!(r.reason, "repeated_language");
        assert_eq!(r.because.as_ref().map(|f| f.id.as_str()), Some("new"));

        let old_phrase = r.evidence.iter().find(|e| e.kind == "shared_phrase" && e.related_id.is_none()).unwrap();
        assert_eq!(old_phrase.detail, "premature judgment", "the evidence is the shared run, not a single term");
        let new_phrase = r.evidence.iter().find(|e| e.related_id.as_deref() == Some("new")).unwrap();
        assert_eq!(new_phrase.detail, "premature judgment");
    }

    #[test]
    fn every_return_carries_evidence() {
        let db = db();
        put(&db, "old", "the tending metaphor is fine", "2023-05-01T10:00:00+00:00");
        put(&db, "new", "back to the tending metaphor", "2026-09-19T16:30:00+00:00");

        let r = db.select_return(NOW).unwrap().unwrap();
        assert!(!r.evidence.is_empty(), "a Return with no evidence is a bug, not a fallback");
        assert!(r.evidence.iter().all(|e| !e.detail.trim().is_empty()));
        assert!(r.because.is_some(), "every return names the act that caused it");
    }

    #[test]
    fn a_word_that_appears_everywhere_is_not_a_recurrence() {
        let db = db();
        for i in 0..8 {
            put(
                &db,
                &format!("f{i}"),
                "onboarding onboarding notes",
                &format!("2023-0{}-01T10:00:00+00:00", i + 1),
            );
        }
        put(&db, "new", "more onboarding notes", "2026-09-19T16:30:00+00:00");
        let r = db.select_return(NOW).unwrap();
        assert!(r.map(|x| x.reason != "repeated_language").unwrap_or(true));
    }

    #[test]
    fn an_unanswered_return_is_what_select_returns_until_it_is_let_go() {
        let db = db();
        put(&db, "old", "premature judgment about folders", "2023-03-14T23:41:00+00:00");
        put(&db, "new", "premature judgment again today", "2026-09-19T16:30:00+00:00");

        let first = db.select_return(NOW).unwrap().unwrap();
        let again = db.select_return("2026-09-19T18:00:00+00:00").unwrap().unwrap();
        assert_eq!(again.id, first.id, "the same unanswered return, not a new one");

        db.record_return_outcome(first.id, "let_go").unwrap();
        assert!(
            db.select_return("2026-09-19T18:00:00+00:00").unwrap().is_none(),
            "after letting go, cooldown still holds"
        );
    }

    #[test]
    fn the_same_fragment_does_not_come_back_within_a_month() {
        let db = db();
        put(&db, "old", "premature judgment about folders", "2023-03-14T23:41:00+00:00");
        put(&db, "new", "premature judgment again today", "2026-09-19T16:30:00+00:00");

        let first = db.select_return(NOW).unwrap().unwrap();
        assert_eq!(first.fragment.id, "old");
        db.record_return_outcome(first.id, "let_go").unwrap();

        let later = db.select_return("2026-09-25T17:00:00+00:00").unwrap();
        assert!(later.is_none(), "the same material must not keep returning");
    }

    #[test]
    fn an_anniversary_is_not_a_return() {
        let db = db();
        put(&db, "old", "the sea was the colour of a bruise", "2024-09-19T12:00:00+00:00");
        put(&db, "unrelated", "milk", "2026-09-19T16:00:00+00:00");

        let r = db.select_return(NOW).unwrap();
        assert!(r.is_none(), "a date alone is not a reason");
    }

    #[test]
    fn held_material_is_not_a_return() {
        let db = db();
        put(&db, "held", "boiler guy — Tues between 12 and 3", "2025-01-04T10:00:00+00:00");
        {
            let conn = db.0.lock().unwrap();
            conn.execute(
                "INSERT INTO holds (fragment_id, held_at) VALUES ('held', '2025-01-04T10:00:00+00:00')",
                [],
            )
            .unwrap();
        }
        put(&db, "unrelated", "milk", "2026-09-19T16:00:00+00:00");
        assert!(db.select_return(NOW).unwrap().is_none());
    }

    #[test]
    fn meeting_a_source_again_brings_back_the_earlier_meeting() {
        let db = db();
        let first = db
            .capture_encounter(
                "https://theatlantic.com/ideas/second-brain/",
                "the second-brain crowd have rebuilt the filing cabinet",
                None,
                None,
                None,
            )
            .unwrap();
        {
            let conn = db.0.lock().unwrap();
            conn.execute(
                "UPDATE fragments SET captured_at = '2024-03-09T10:00:00+00:00' WHERE id = ?1",
                [&first.id],
            )
            .unwrap();
            conn.execute(
                "UPDATE encounters SET shared_at = '2024-03-09T10:00:00+00:00' WHERE fragment_id = ?1",
                [&first.id],
            )
            .unwrap();
        }
        let again = db
            .capture_encounter(
                "https://www.theatlantic.com/ideas/second-brain/?utm_source=newsletter",
                "",
                Some("com.apple.Safari"),
                None,
                None,
            )
            .unwrap();
        {
            let conn = db.0.lock().unwrap();
            conn.execute(
                "UPDATE fragments SET captured_at = '2026-09-19T16:48:00+00:00' WHERE id = ?1",
                [&again.id],
            )
            .unwrap();
            conn.execute(
                "UPDATE encounters SET shared_at = '2026-09-19T16:48:00+00:00' WHERE fragment_id = ?1",
                [&again.id],
            )
            .unwrap();
        }

        let r = db.select_return(NOW).unwrap().expect("a repeat encounter is a ground");
        assert_eq!(r.reason, "same_source");
        assert_eq!(r.fragment.id, first.id, "the EARLIER meeting is what returns");
        assert_eq!(r.because.as_ref().map(|f| f.id.as_str()), Some(again.id.as_str()));

        let url = r.evidence.iter().find(|e| e.kind == "url").unwrap();
        assert!(url.detail.contains("theatlantic.com"), "the evidence is the actual source");
    }

    #[test]
    fn a_source_met_once_is_not_a_return() {
        let db = db();
        let only = db
            .capture_encounter("https://example.com/only", "words", None, None, None)
            .unwrap();
        let _ = only;
        let r = db.select_return(NOW).unwrap();
        assert!(r.map(|x| x.reason != "same_source").unwrap_or(true));
    }

    #[test]
    fn a_quiet_line_returns_with_the_last_addition_as_the_reason() {
        let db = db();
        let first = db.capture_fragment("a notebook that doesn't ask", "typed", None).unwrap();
        set_captured(&db, &first.id, "2024-03-01T10:00:00+00:00");
        let second = db
            .continue_fragment(&first.id, "same problem with tags", "typed", None, false)
            .unwrap();
        set_captured(&db, &second.id, "2024-11-02T08:19:00+00:00");
        let third = db
            .continue_fragment(&second.id, "deciding what it is before I'm done", "typed", None, false)
            .unwrap();
        set_captured(&db, &third.id, "2025-03-14T23:41:00+00:00");

        let r = db.select_return(NOW).unwrap().expect("a quiet line is a ground");
        assert_eq!(r.reason, "line_continued");
        assert_eq!(r.fragment.id, first.id);
        assert_eq!(r.because.as_ref().map(|f| f.id.as_str()), Some(third.id.as_str()));
        let c = r.evidence.iter().find(|e| e.kind == "continuation").unwrap();
        assert!(c.detail.contains("deciding what it is before"), "the last addition is the quote");
    }

    #[test]
    fn outcomes_are_recorded_and_letting_go_is_not_a_delete() {
        let db = db();
        put(&db, "old", "premature judgment about folders", "2023-03-14T23:41:00+00:00");
        put(&db, "new", "premature judgment again today", "2026-09-19T16:30:00+00:00");

        let r = db.select_return(NOW).unwrap().unwrap();
        db.record_return_outcome(r.id, "let_go").unwrap();

        let conn = db.0.lock().unwrap();
        let (outcome, at): (String, String) = conn
            .query_row("SELECT outcome, outcome_at FROM returns WHERE id = ?1", [r.id], |x| {
                Ok((x.get(0)?, x.get(1)?))
            })
            .unwrap();
        assert_eq!(outcome, "let_go");
        assert!(!at.is_empty());

        let body: String = conn
            .query_row("SELECT body FROM fragments WHERE id = 'old'", [], |x| x.get(0))
            .unwrap();
        assert_eq!(body, "premature judgment about folders");
    }

    #[test]
    fn a_return_is_never_caused_by_material_the_person_has_not_touched_recently() {
        let db = db();
        put(&db, "old", "premature judgment about folders", "2023-03-14T23:41:00+00:00");
        put(&db, "new", "premature judgment again", "2026-09-16T10:00:00+00:00");

        let r = db.select_return(NOW).unwrap();
        assert!(
            r.map(|x| x.reason != "repeated_language").unwrap_or(true),
            "repeated language must be triggered by something written now"
        );
    }
}
