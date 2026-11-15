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
const MIN_RETURN_AGE_DAYS: i64 = 7;
const CAUSE_WINDOW_HOURS: i64 = 48;
// Bound background work under the shared SQLite lock.
const MAX_RECENT_CAUSES: i64 = 32;

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
    "about",
    "after",
    "again",
    "always",
    "another",
    "because",
    "before",
    "being",
    "between",
    "could",
    "every",
    "first",
    "going",
    "might",
    "never",
    "other",
    "really",
    "should",
    "since",
    "still",
    "their",
    "there",
    "these",
    "thing",
    "think",
    "those",
    "through",
    "under",
    "where",
    "which",
    "while",
    "would",
    "thought",
    "something",
    "anything",
];

/// The prototype's stoplist — used by the shared-run matcher, not by FTS.
const RUN_STOP: &[&str] = &[
    "the", "a", "an", "and", "or", "but", "of", "to", "in", "on", "at", "for", "with", "is", "are",
    "was", "were", "be", "been", "it", "its", "this", "that", "these", "those", "i", "me", "my",
    "we", "you", "your", "he", "she", "they", "them", "his", "her", "our", "their", "as", "by",
    "from", "not", "no", "so", "if", "then", "than", "too", "very", "just", "about", "into",
    "over", "under", "again", "there", "here", "what", "which", "who", "whom", "when", "where",
    "why", "how", "do", "does", "did", "done", "have", "has", "had", "having", "um", "uh", "ok",
    "okay", "i'm", "i've", "it's", "that's", "don't",
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
    if !s.chars().all(|c| c.is_ascii_lowercase() || c == '\'') { return s; }
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
                if cur[j] >= 3 && best.as_ref().map(|b| b.length).unwrap_or(0) < cur[j] {
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
        .take(160)
        .map(|t| t.trim_matches('\'').to_lowercase())
        .filter(|t| t.chars().count() >= 5 && !STOPWORDS.contains(&t.as_str()))
        .collect();
    terms.sort();
    terms.dedup();
    terms.truncate(12);
    terms
}

fn elapsed_ms(at: &str, now: &str) -> Option<i64> {
    let at = chrono::DateTime::parse_from_rfc3339(at).ok()?;
    let now = chrono::DateTime::parse_from_rfc3339(now).ok()?;
    Some((now - at).num_milliseconds())
}
fn recent(at: &str, now: &str) -> bool {
    elapsed_ms(at, now).is_some_and(|ms| (0..=CAUSE_WINDOW_HOURS * 3_600_000).contains(&ms))
}
fn eligible(conn: &Connection, id: &str, at: &str, now: &str) -> Result<bool, rusqlite::Error> {
    if !elapsed_ms(at, now).is_some_and(|ms| ms >= MIN_RETURN_AGE_DAYS * 86_400_000) {
        return Ok(false);
    }
    let available: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM fragments f WHERE f.id = ?1 AND f.removed_at IS NULL AND TRIM(f.body) <> ''
         AND NOT EXISTS(SELECT 1 FROM holds h WHERE h.fragment_id = f.id AND h.released_at IS NULL))",
        [id], |r| r.get(0))?;
    if !available {
        return Ok(false);
    }
    let last: Option<String> = conn.query_row(
        "SELECT surfaced_at FROM returns WHERE fragment_id = ?1 ORDER BY surfaced_at DESC LIMIT 1",
        [id], |r| r.get(0)).optional()?;
    Ok(last.is_none_or(|at| {
        elapsed_ms(&at, now).is_some_and(|ms| ms >= SAME_FRAGMENT_COOLDOWN_DAYS * 86_400_000)
    }))
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
            let value = load_return(&conn, open_id)?;
            if valid_open_return(&conn, &value)? {
                return Ok(Some(value));
            }
            conn.execute(
                "UPDATE returns SET outcome = 'expired', outcome_at = ?1 WHERE id = ?2",
                rusqlite::params![now, open_id],
            )?;
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
            if elapsed_ms(&last, now).is_none_or(|ms| ms < RETURN_COOLDOWN_HOURS * 3_600_000) {
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

    /// Whether a Return is already waiting for an answer.
    ///
    /// Read-only on purpose, and that is the whole point of it existing next to
    /// [`Db::select_return`]: selecting *surfaces* a Return, inserting a row and spending
    /// the twenty-hour cooldown. The menu-bar glyph asks this question on every refresh and
    /// must never be the thing that decides a Return happened — a modifier nobody was
    /// present for would be the product telling you about a decision it made on your
    /// behalf. The Record and the panel surface Returns; this only reports one.
    pub fn return_waiting(&self) -> Result<bool, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let open: Option<i64> = conn
            .query_row(
                "SELECT id FROM returns WHERE outcome IS NULL LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        Ok(open.is_some())
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
    let newest: Vec<(String, String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT id, body, captured_at FROM fragments WHERE removed_at IS NULL
             AND julianday(captured_at) BETWEEN julianday(?1, '-2 days') AND julianday(?1)
             ORDER BY captured_at DESC LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![now, MAX_RECENT_CAUSES], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?
            .collect::<Result<_, _>>()?;
        rows
    };
    for (newest_id, body, newest_at) in newest {
        if !recent(&newest_at, now) {
            continue;
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
                "SELECT f.id, f.body, f.captured_at FROM fragments_fts t
                 JOIN fragments f ON f.id = t.fragment_id WHERE fragments_fts MATCH ?1
                 AND f.id <> ?2 AND f.removed_at IS NULL ORDER BY f.captured_at ASC LIMIT 12",
            )?;
            let rows = stmt.query_map(rusqlite::params![&quoted, &newest_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?;
            for row in rows {
                let (old_id, old_body, old_at) = row?;
                if !eligible(conn, &old_id, &old_at, now)? {
                    continue;
                }
                if let Some(run) = shared_run(&tokenize(&old_body), &newest_tokens) {
                    if best.as_ref().map(|(_, r)| r.length).unwrap_or(0) < run.length {
                        best = Some((old_id, run));
                    }
                }
            }
        }
        if let Some((old_id, run)) = best {
            return Ok(Some((
                old_id,
                "repeated_language".into(),
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
            )));
        }
    }
    Ok(None)
}

/// "You encountered this URL again", with both dates.
///
/// Reads `encounters.url_key`, which was written at capture. Nothing is matched by
/// similarity here — two encounters either share a stored key or they do not.
fn same_source(conn: &Connection, now: &str) -> Result<Candidate, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT f.id, f.captured_at, e.url_key, e.url_raw FROM fragments f
         JOIN encounters e ON e.fragment_id = f.id WHERE f.removed_at IS NULL
         AND julianday(f.captured_at) BETWEEN julianday(?1, '-2 days') AND julianday(?1)
         ORDER BY f.captured_at DESC LIMIT ?2",
    )?;
    let newest = stmt.query_map(rusqlite::params![now, MAX_RECENT_CAUSES], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
        ))
    })?;
    for row in newest {
        let (newest_id, newest_at, key, raw) = row?;
        if !recent(&newest_at, now) {
            continue;
        }
        let Some(domain) = domain_of(&raw) else {
            continue;
        };
        let mut earlier = conn.prepare(
            "SELECT f.id, f.captured_at FROM encounters e JOIN fragments f ON f.id = e.fragment_id
             WHERE e.url_key = ?1 AND f.id <> ?2 AND f.removed_at IS NULL ORDER BY f.captured_at ASC LIMIT 64")?;
        let candidates = earlier.query_map(rusqlite::params![key, newest_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        for candidate in candidates {
            let (id, at) = candidate?;
            if !eligible(conn, &id, &at, now)? {
                continue;
            }
            return Ok(Some((
                id,
                "same_source".into(),
                vec![ReturnEvidence {
                    kind: "url".into(),
                    detail: domain,
                    occurred_at: Some(newest_at),
                    related_id: Some(newest_id),
                }],
            )));
        }
    }
    Ok(None)
}

/// A recent addition is the cause; a quiet old Line alone never asks for attention.
fn line_continued(conn: &Connection, now: &str) -> Result<Candidate, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT root.id, root.captured_at, child.id, child.body, child.captured_at
         FROM line_index li JOIN fragments child ON child.id = li.fragment_id
         JOIN fragments root ON root.id = li.root_id WHERE li.fragment_id <> li.root_id
         AND root.removed_at IS NULL AND child.removed_at IS NULL
         AND julianday(child.captured_at) BETWEEN julianday(?1, '-2 days') AND julianday(?1)
         ORDER BY child.captured_at DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(rusqlite::params![now, MAX_RECENT_CAUSES], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
        ))
    })?;
    for row in rows {
        let (id, at, cause_id, body, cause_at) = row?;
        if !recent(&cause_at, now) || !eligible(conn, &id, &at, now)? {
            continue;
        }
        let snippet = body
            .split_whitespace()
            .take(12)
            .collect::<Vec<_>>()
            .join(" ");
        if snippet.is_empty() {
            continue;
        }
        return Ok(Some((
            id,
            "line_continued".into(),
            vec![ReturnEvidence {
                kind: "continuation".into(),
                detail: snippet,
                occurred_at: Some(cause_at),
                related_id: Some(cause_id),
            }],
        )));
    }
    Ok(None)
}

fn valid_open_return(conn: &Connection, value: &Return) -> Result<bool, rusqlite::Error> {
    let Some(cause) = &value.because else {
        return Ok(false);
    };
    let live: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM fragments WHERE id = ?1 AND removed_at IS NULL)
         AND EXISTS(SELECT 1 FROM fragments WHERE id = ?2 AND removed_at IS NULL)
         AND NOT EXISTS(SELECT 1 FROM holds WHERE fragment_id = ?1 AND released_at IS NULL)",
        rusqlite::params![value.fragment.id, cause.id],
        |r| r.get(0),
    )?;
    if !live {
        return Ok(false);
    }
    Ok(match value.reason.as_str() {
        "repeated_language" => shared_run(&tokenize(&value.fragment.body), &tokenize(&cause.body))
            .is_some_and(|run| {
                value
                    .evidence
                    .iter()
                    .any(|e| e.related_id.is_none() && e.detail == run.a)
                    && value.evidence.iter().any(|e| {
                        e.related_id.as_deref() == Some(cause.id.as_str()) && e.detail == run.b
                    })
            }),
        "same_source" => conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM encounters a JOIN encounters b ON a.url_key = b.url_key
             WHERE a.fragment_id = ?1 AND b.fragment_id = ?2)",
            rusqlite::params![value.fragment.id, cause.id],
            |r| r.get(0),
        )?,
        "line_continued" => {
            let snippet = cause.body.split_whitespace().take(12).collect::<Vec<_>>().join(" ");
            let same_line: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM line_index a JOIN line_index b ON a.root_id = b.root_id
                 WHERE a.fragment_id = ?1 AND b.fragment_id = ?2)",
                rusqlite::params![value.fragment.id, cause.id], |r| r.get(0))?;
            same_line && value.evidence.iter().any(|e| e.kind == "continuation" && e.detail == snippet)
        },
        _ => false,
    })
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

    /// The property the menu-bar glyph depends on.
    ///
    /// The modifier is drawn from `return_waiting`, which is refreshed on every write and
    /// every time the panel is toggled. If asking that question could surface a Return, the
    /// menu bar would be deciding — on a timer, with nobody present — that older material
    /// came back, and it would spend the cooldown doing it.
    #[test]
    fn asking_whether_a_return_waits_never_creates_one() {
        let db = db();
        put(&db, "a", "quiet premature judgment again", "2026-09-19T16:00:00+00:00");
        put(&db, "b", "quiet premature judgment", "2025-01-04T10:00:00+00:00");

        // A Return is genuinely available here — the same fixture the selecting test uses.
        assert!(!db.return_waiting().unwrap());
        assert!(!db.return_waiting().unwrap());
        assert_eq!(count_returns(&db), 0);

        // And selecting still finds it, so the fixture was not the reason for the silence.
        assert!(db.select_return(NOW).unwrap().is_some());
        assert_eq!(count_returns(&db), 1);
    }

    #[test]
    fn a_return_waits_until_it_is_answered() {
        let db = db();
        put(&db, "a", "quiet premature judgment again", "2026-09-19T16:00:00+00:00");
        put(&db, "b", "quiet premature judgment", "2025-01-04T10:00:00+00:00");

        let surfaced = db.select_return(NOW).unwrap().expect("a return");
        assert!(db.return_waiting().unwrap());

        // Letting go is an outcome, not a deletion: the row stays and stops waiting.
        db.record_return_outcome(surfaced.id, "let_go").unwrap();
        assert!(!db.return_waiting().unwrap());
        assert_eq!(count_returns(&db), 1);
    }

    /// The panel and the window answer the same row. Opening the panel while a Return is
    /// already waiting must hand back that one rather than starting a second.
    #[test]
    fn both_surfaces_are_given_the_same_return() {
        let db = db();
        put(&db, "a", "quiet premature judgment again", "2026-09-19T16:00:00+00:00");
        put(&db, "b", "quiet premature judgment", "2025-01-04T10:00:00+00:00");

        let window = db.select_return(NOW).unwrap().expect("a return");
        let panel = db.select_return(NOW).unwrap().expect("the same return");
        assert_eq!(window.id, panel.id);
        assert_eq!(count_returns(&db), 1);
    }

    fn count_returns(db: &Db) -> i64 {
        let conn = db.0.lock().unwrap();
        conn.query_row("SELECT COUNT(*) FROM returns", [], |r| r.get(0))
            .unwrap()
    }

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
        put(&db, "a", "quiet premature judgment again", "2026-09-19T16:00:00+00:00");
        put(&db, "b", "quiet premature judgment", "2026-09-15T10:00:00+00:00");
        assert!(db.select_return(NOW).unwrap().is_none());
    }

    #[test]
    fn a_phrase_used_again_today_brings_back_the_older_moment_with_its_words() {
        let db = db();
        put(&db, "old", "filing is a kind of quiet premature judgment", "2023-03-14T23:41:00+00:00");
        put(&db, "new", "still doing quiet premature judgment on every note", "2026-09-19T16:30:00+00:00");

        let r = db.select_return(NOW).unwrap().expect("a recurrence should return");
        assert_eq!(r.fragment.id, "old", "the OLDER moment is what comes back");
        assert_eq!(r.reason, "repeated_language");
        assert_eq!(r.because.as_ref().map(|f| f.id.as_str()), Some("new"));

        let old_phrase = r.evidence.iter().find(|e| e.kind == "shared_phrase" && e.related_id.is_none()).unwrap();
        assert_eq!(old_phrase.detail, "quiet premature judgment", "the evidence is the shared run, not a single term");
        let new_phrase = r.evidence.iter().find(|e| e.related_id.as_deref() == Some("new")).unwrap();
        assert_eq!(new_phrase.detail, "quiet premature judgment");
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
        put(&db, "old", "quiet premature judgment about folders", "2023-03-14T23:41:00+00:00");
        put(&db, "new", "quiet premature judgment again today", "2026-09-19T16:30:00+00:00");

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
        put(&db, "old", "quiet premature judgment about folders", "2023-03-14T23:41:00+00:00");
        put(&db, "new", "quiet premature judgment again today", "2026-09-19T16:30:00+00:00");

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
    fn a_recent_addition_returns_an_older_line_with_its_words() {
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
        set_captured(&db, &third.id, "2026-09-19T16:00:00+00:00");

        let r = db.select_return(NOW).unwrap().expect("a quiet line is a ground");
        assert_eq!(r.reason, "line_continued");
        assert_eq!(r.fragment.id, first.id);
        assert_eq!(r.because.as_ref().map(|f| f.id.as_str()), Some(third.id.as_str()));
        let c = r.evidence.iter().find(|e| e.kind == "continuation").unwrap();
        assert!(c.detail.contains("deciding what it is before"), "the last addition is the quote");
        db.correct_fragment(&third.id, "new wording").unwrap();
        assert!(db.select_return(NOW).unwrap().is_none(), "a corrected-away quote must not remain");
    }

    #[test]
    fn outcomes_are_recorded_and_letting_go_is_not_a_delete() {
        let db = db();
        put(&db, "old", "quiet premature judgment about folders", "2023-03-14T23:41:00+00:00");
        put(&db, "new", "quiet premature judgment again today", "2026-09-19T16:30:00+00:00");

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
        assert_eq!(body, "quiet premature judgment about folders");
    }

    #[test]
    fn a_return_is_never_caused_by_material_the_person_has_not_touched_recently() {
        let db = db();
        put(&db, "old", "quiet premature judgment about folders", "2023-03-14T23:41:00+00:00");
        put(&db, "new", "quiet premature judgment again", "2026-09-16T10:00:00+00:00");

        let r = db.select_return(NOW).unwrap();
        assert!(
            r.map(|x| x.reason != "repeated_language").unwrap_or(true),
            "repeated language must be triggered by something written now"
        );
    }
    #[test]
    fn a_return_starts_at_seven_full_days_with_a_recent_cause() {
        for (old_at, cause_at, expected) in [
            ("2026-09-12T17:00:00.001Z", "2026-09-19T17:00:00Z", false),
            ("2026-09-12T17:00:00Z", "2026-09-19T17:00:00Z", true),
            ("2026-09-12T17:00:00Z", "2026-09-17T17:00:00Z", true),
            ("2026-09-12T17:00:00Z", "2026-09-17T16:59:59.999Z", false),
            ("2026-09-12T17:00:00Z", "2026-09-19T17:00:00.001Z", false),
        ] {
            let db = db();
            put(&db, "old", "quiet premature judgment", old_at);
            put(&db, "cause", "quiet premature judgment again", cause_at);
            assert_eq!(
                db.select_return(NOW).unwrap().is_some(),
                expected,
                "{old_at}, {cause_at}"
            );
        }
    }
    #[test]
    fn an_unrelated_latest_capture_does_not_hide_recent_evidence() {
        let db = db();
        put(
            &db,
            "old",
            "quiet premature judgment",
            "2026-09-12T17:00:00Z",
        );
        put(
            &db,
            "cause",
            "quiet premature judgment again",
            "2026-09-19T16:00:00Z",
        );
        put(&db, "latest", "milk", "2026-09-19T16:30:00Z");
        assert_eq!(
            db.select_return(NOW).unwrap().unwrap().because.unwrap().id,
            "cause"
        );
    }
    #[test]
    fn held_or_recently_returned_candidates_do_not_starve_another_eligible_moment() {
        let db = db();
        put(
            &db,
            "older",
            "quiet premature judgment",
            "2025-01-01T10:00:00Z",
        );
        put(
            &db,
            "other",
            "quiet premature judgment",
            "2025-02-01T10:00:00Z",
        );
        put(
            &db,
            "cause",
            "quiet premature judgment again",
            "2026-09-19T16:00:00Z",
        );
        db.hold_fragment("older").unwrap();
        let value = db.select_return(NOW).unwrap().unwrap();
        assert_eq!(value.fragment.id, "other");
        db.record_return_outcome(value.id, "let_go").unwrap();
        db.release_fragment("older").unwrap();
        db.0.lock()
            .unwrap()
            .execute(
                "UPDATE returns SET surfaced_at = '2026-09-17T16:00:00Z'",
                [],
            )
            .unwrap();
        let value = db.select_return(NOW).unwrap().unwrap();
        assert_eq!(value.fragment.id, "older");
    }
    #[test]
    fn an_open_return_expires_when_its_quoted_evidence_is_corrected_away() {
        let db = db();
        put(
            &db,
            "old",
            "quiet premature judgment",
            "2025-01-01T10:00:00Z",
        );
        put(
            &db,
            "cause",
            "quiet premature judgment again",
            "2026-09-19T16:00:00Z",
        );
        let value = db.select_return(NOW).unwrap().unwrap();
        db.correct_fragment("cause", "something unrelated").unwrap();
        assert!(db.select_return(NOW).unwrap().is_none());
        let outcome: String =
            db.0.lock()
                .unwrap()
                .query_row(
                    "SELECT outcome FROM returns WHERE id = ?1",
                    [value.id],
                    |r| r.get(0),
                )
                .unwrap();
        assert_eq!(outcome, "expired");
    }
    #[test]
    fn an_old_quiet_line_has_no_recent_cause() {
        let db = db();
        let root = db.capture_fragment("an older idea", "typed", None).unwrap();
        set_captured(&db, &root.id, "2025-01-01T10:00:00Z");
        let next = db
            .continue_fragment(&root.id, "another point", "typed", None, false)
            .unwrap();
        set_captured(&db, &next.id, "2025-02-01T10:00:00Z");
        assert!(db.select_return(NOW).unwrap().is_none());
    }
    #[test]
    fn shared_mobile_policy_cases() {
        let cases: Vec<serde_json::Value> = serde_json::from_str(include_str!("return_policy_cases.json")).unwrap();
        let now = chrono::DateTime::parse_from_rfc3339(NOW).unwrap();
        for case in cases {
            let db = db();
            let old = now - chrono::Duration::milliseconds(case["oldAgeMs"].as_i64().unwrap());
            let cause = now - chrono::Duration::milliseconds(case["causeAgeMs"].as_i64().unwrap());
            put(&db, "old", "quiet premature judgment", &old.to_rfc3339());
            put(&db, "cause", case["body"].as_str().unwrap_or("quiet premature judgment again"), &cause.to_rfc3339());
            if case["held"].as_bool().unwrap_or(false) { db.hold_fragment("old").unwrap(); }
            assert_eq!(db.select_return(NOW).unwrap().is_some(), case["expected"].as_bool().unwrap(), "{}", case["name"]);
        }
    }

}
