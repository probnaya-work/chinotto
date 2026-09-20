//! External material and voice: what arrived, and what was later derived from it.
//!
//! One rule governs this whole module: **preserve what was observed, derive the rest, and
//! never confuse the two.**
//!
//!   * The URL is stored exactly as received, before any canonicalisation. Title, canonical
//!     URL, site name and extracted text are fetched later, may fail, and may change.
//!   * `source_app` is recorded only when the share path actually supplied one. There is no
//!     inference from the URL, no "probably Safari", no default.
//!   * The audio is the voice fragment. The transcript is derived from it and may be absent,
//!     pending, or failed — none of which makes the fragment incomplete.
//!   * Correcting a transcript edits the fragment body and its revision history. It never
//!     touches the audio and never rewrites what the machine actually heard.
//!
//! Nothing in here can block capture. Every enrichment path is separate from the write that
//! created the fragment.

use super::record::{sync_fts, Fragment};
use super::Db;
use rusqlite::OptionalExtension;

/// Query parameters that identify a campaign rather than a page.
const TRACKING_PARAMS: &[&str] = &[
    "utm_source", "utm_medium", "utm_campaign", "utm_term", "utm_content", "utm_id", "gclid",
    "fbclid", "mc_cid", "mc_eid", "igshid", "ref", "ref_src", "spm", "cmpid", "ncid",
];

/// A deterministic, offline identity for a source.
///
/// Two shares of the same article should be recognised as the same encounter even when one
/// came through a newsletter with tracking parameters attached. This is computed once at
/// capture and stored, so repeat detection reads a durable key rather than re-deriving a
/// match later with fuzzy rules.
///
/// It never replaces `url_raw`, which is kept byte-for-byte.
pub fn url_key(raw: &str) -> String {
    let trimmed = raw.trim();
    let without_scheme = trimmed
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(trimmed);
    // Drop the fragment: it addresses a place within a page, not a different page.
    let without_fragment = without_scheme.split('#').next().unwrap_or("");
    let (path_part, query_part) = match without_fragment.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (without_fragment, None),
    };

    let mut host_and_path = path_part.trim_end_matches('/').to_lowercase();
    if let Some(rest) = host_and_path.strip_prefix("www.") {
        host_and_path = rest.to_string();
    }

    let kept: Vec<String> = query_part
        .map(|q| {
            let mut params: Vec<String> = q
                .split('&')
                .filter(|kv| !kv.is_empty())
                .filter(|kv| {
                    let name = kv.split('=').next().unwrap_or("").to_lowercase();
                    !TRACKING_PARAMS.contains(&name.as_str())
                })
                .map(|kv| kv.to_string())
                .collect();
            // Order of query parameters does not change which page you landed on.
            params.sort();
            params
        })
        .unwrap_or_default();

    if kept.is_empty() {
        host_and_path
    } else {
        format!("{host_and_path}?{}", kept.join("&"))
    }
}

/// Host, for display. Derived, but purely local — available offline and on first paint.
pub fn domain_of(raw: &str) -> Option<String> {
    let without_scheme = raw.trim().split_once("://").map(|(_, r)| r).unwrap_or(raw.trim());
    let host = without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .rsplit('@')
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("");
    let host = host.strip_prefix("www.").unwrap_or(host);
    if host.is_empty() || !host.contains('.') {
        None
    } else {
        Some(host.to_lowercase())
    }
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Encounter {
    pub id: i64,
    pub fragment_id: String,
    /// Exactly as received.
    pub url_raw: String,
    pub url_key: String,
    pub source_app: Option<String>,
    pub shared_at: String,
    pub selected_text: Option<String>,
    // --- everything below is derived and may be absent ---
    pub url_canonical: Option<String>,
    pub domain: Option<String>,
    pub title: Option<String>,
    pub site_name: Option<String>,
    /// "pending" | "ok" | "failed"
    pub enrichment_state: String,
    pub fetched_at: Option<String>,
    pub failure: Option<String>,
    /// How many other fragments have met this same source.
    pub times_met: i64,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceCapture {
    pub fragment_id: String,
    pub audio_path: String,
    pub duration_ms: i64,
    pub recorded_at: String,
    /// The audio was looked for and is not there. The fragment still exists.
    pub audio_missing: bool,
    // --- derived ---
    pub machine_transcript: Option<String>,
    /// "pending" | "ok" | "failed"
    pub transcript_state: String,
    pub transcribed_at: Option<String>,
    pub failure: Option<String>,
    /// True when the body no longer matches what the machine heard.
    pub transcript_corrected: bool,
}

/// Everything a page of the Record needs to render its material.
pub type PageMaterials = (Vec<Encounter>, Vec<VoiceCapture>);

/// What a metadata fetch came back with: title, canonical URL, site name — any of which the
/// page may simply not have offered. `Err` carries why the attempt failed.
pub type FetchedMeta = Result<(Option<String>, Option<String>, Option<String>), String>;

impl Db {
    /// Captures external material. Local, immediate, and complete on its own.
    ///
    /// `body` is the person's own words and may be empty — a naked URL is a valid fragment,
    /// and the design shows exactly that. No network call happens on this path, so sharing
    /// a link works identically offline.
    pub fn capture_encounter(
        &self,
        url_raw: &str,
        body: &str,
        source_app: Option<&str>,
        selected_text: Option<&str>,
        capture_origin: Option<&str>,
    ) -> Result<Fragment, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO fragments (id, body, captured_at, capture_method, capture_origin) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                id,
                body,
                now,
                if source_app.is_some() { "shared" } else { "url" },
                capture_origin
            ],
        )?;
        conn.execute(
            "INSERT INTO encounters (fragment_id, url_raw, url_key, source_app, shared_at, selected_text) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![id, url_raw, url_key(url_raw), source_app, now, selected_text],
        )?;
        let encounter_id = conn.last_insert_rowid();

        // The domain is readable without the network, so the fragment is never a bare string
        // even offline. Everything else stays pending until something fetches it.
        conn.execute(
            "INSERT INTO encounter_enrichment (encounter_id, domain, state) VALUES (?1, ?2, 'pending')",
            rusqlite::params![encounter_id, domain_of(url_raw)],
        )?;

        sync_fts(&conn, &id)?;
        conn.query_row(
            "SELECT id, body, captured_at, capture_method, capture_origin, corrected_at, \
             correction_count, legacy_edit_count FROM fragments WHERE id = ?1",
            [&id],
            |r| {
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
            },
        )
    }

    /// Records the result of trying to fetch metadata. Success or failure, both are states.
    pub fn record_enrichment(
        &self,
        encounter_id: i64,
        result: FetchedMeta,
    ) -> Result<(), rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        match result {
            Ok((title, canonical, site_name)) => {
                conn.execute(
                    "UPDATE encounter_enrichment SET title = ?1, url_canonical = ?2, site_name = ?3, \
                     state = 'ok', attempted_at = ?4, fetched_at = ?4, failure = NULL \
                     WHERE encounter_id = ?5",
                    rusqlite::params![title, canonical, site_name, now, encounter_id],
                )?;
            }
            Err(reason) => {
                // A failure is recorded once, on the fragment it concerns, and does not
                // retry itself into a loop.
                conn.execute(
                    "UPDATE encounter_enrichment SET state = 'failed', attempted_at = ?1, failure = ?2 \
                     WHERE encounter_id = ?3",
                    rusqlite::params![now, reason, encounter_id],
                )?;
            }
        }
        // The title is searchable once it exists.
        let fragment_id: Option<String> = conn
            .query_row(
                "SELECT fragment_id FROM encounters WHERE id = ?1",
                [encounter_id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(fid) = fragment_id {
            sync_fts(&conn, &fid)?;
        }
        Ok(())
    }

    /// Encounters whose metadata has never been fetched. Drives the enrichment queue.
    pub fn encounters_awaiting_enrichment(&self, limit: i64) -> Result<Vec<(i64, String)>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT e.id, e.url_raw FROM encounters e \
             JOIN encounter_enrichment x ON x.encounter_id = e.id \
             WHERE x.state = 'pending' ORDER BY e.shared_at DESC LIMIT ?1",
        )?;
        let out: Vec<(i64, String)> = stmt
            .query_map([limit], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_, _>>()?;
        Ok(out)
    }

    pub fn encounter_for(&self, fragment_id: &str) -> Result<Option<Encounter>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        conn.query_row(
            "SELECT e.id, e.fragment_id, e.url_raw, e.url_key, e.source_app, e.shared_at, e.selected_text, \
                    x.url_canonical, x.domain, x.title, x.site_name, \
                    COALESCE(x.state, 'pending'), x.fetched_at, x.failure, \
                    (SELECT COUNT(DISTINCT e2.fragment_id) FROM encounters e2 WHERE e2.url_key = e.url_key) \
             FROM encounters e LEFT JOIN encounter_enrichment x ON x.encounter_id = e.id \
             WHERE e.fragment_id = ?1 ORDER BY e.id LIMIT 1",
            [fragment_id],
            |r| {
                Ok(Encounter {
                    id: r.get(0)?,
                    fragment_id: r.get(1)?,
                    url_raw: r.get(2)?,
                    url_key: r.get(3)?,
                    source_app: r.get(4)?,
                    shared_at: r.get(5)?,
                    selected_text: r.get(6)?,
                    url_canonical: r.get(7)?,
                    domain: r.get(8)?,
                    title: r.get(9)?,
                    site_name: r.get(10)?,
                    enrichment_state: r.get(11)?,
                    fetched_at: r.get(12)?,
                    failure: r.get(13)?,
                    times_met: r.get(14)?,
                })
            },
        )
        .optional()
    }

    /// Material for a page of fragments, in one pass.
    ///
    /// The Record renders a screenful at a time, and asking per fragment would mean a query
    /// per row — fine at twenty fragments, not at fifty thousand.
    pub fn materials_for(
        &self,
        ids: &[String],
    ) -> Result<PageMaterials, rusqlite::Error> {
        if ids.is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }
        let placeholders = vec!["?"; ids.len()].join(",");
        let params = rusqlite::params_from_iter(ids.iter());

        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(&format!(
            "SELECT e.id, e.fragment_id, e.url_raw, e.url_key, e.source_app, e.shared_at, e.selected_text, \
                    x.url_canonical, x.domain, x.title, x.site_name, \
                    COALESCE(x.state, 'pending'), x.fetched_at, x.failure, \
                    (SELECT COUNT(DISTINCT e2.fragment_id) FROM encounters e2 WHERE e2.url_key = e.url_key) \
             FROM encounters e LEFT JOIN encounter_enrichment x ON x.encounter_id = e.id \
             WHERE e.fragment_id IN ({placeholders})"
        ))?;
        let encounters: Vec<Encounter> = stmt
            .query_map(params, |r| {
                Ok(Encounter {
                    id: r.get(0)?,
                    fragment_id: r.get(1)?,
                    url_raw: r.get(2)?,
                    url_key: r.get(3)?,
                    source_app: r.get(4)?,
                    shared_at: r.get(5)?,
                    selected_text: r.get(6)?,
                    url_canonical: r.get(7)?,
                    domain: r.get(8)?,
                    title: r.get(9)?,
                    site_name: r.get(10)?,
                    enrichment_state: r.get(11)?,
                    fetched_at: r.get(12)?,
                    failure: r.get(13)?,
                    times_met: r.get(14)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        drop(stmt);

        let mut stmt2 = conn.prepare(&format!(
            "SELECT v.fragment_id, v.audio_path, v.duration_ms, v.recorded_at, v.audio_missing, \
                    t.machine_transcript, COALESCE(t.state, 'pending'), t.transcribed_at, t.failure, f.body \
             FROM voice_captures v \
             LEFT JOIN voice_transcripts t ON t.fragment_id = v.fragment_id \
             JOIN fragments f ON f.id = v.fragment_id \
             WHERE v.fragment_id IN ({placeholders})"
        ))?;
        let voices: Vec<VoiceCapture> = stmt2
            .query_map(rusqlite::params_from_iter(ids.iter()), |r| {
                let machine: Option<String> = r.get(5)?;
                let body: String = r.get(9)?;
                Ok(VoiceCapture {
                    fragment_id: r.get(0)?,
                    audio_path: r.get(1)?,
                    duration_ms: r.get(2)?,
                    recorded_at: r.get(3)?,
                    audio_missing: r.get::<_, i64>(4)? != 0,
                    transcript_corrected: match &machine {
                        Some(m) => !m.is_empty() && m != &body,
                        None => false,
                    },
                    machine_transcript: machine,
                    transcript_state: r.get(6)?,
                    transcribed_at: r.get(7)?,
                    failure: r.get(8)?,
                })
            })?
            .collect::<Result<_, _>>()?;

        Ok((encounters, voices))
    }

    /// Every other time this same source was met, oldest first.
    ///
    /// Reads `url_key`, which was stored at capture — not a similarity search run now.
    pub fn same_source_encounters(
        &self,
        fragment_id: &str,
    ) -> Result<Vec<(String, String, Option<String>)>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let key: Option<String> = conn
            .query_row(
                "SELECT url_key FROM encounters WHERE fragment_id = ?1 LIMIT 1",
                [fragment_id],
                |r| r.get(0),
            )
            .optional()?;
        let Some(key) = key else {
            return Ok(Vec::new());
        };

        let mut stmt = conn.prepare(
            "SELECT e.fragment_id, e.shared_at, f.body FROM encounters e \
             JOIN fragments f ON f.id = e.fragment_id \
             WHERE e.url_key = ?1 AND e.fragment_id <> ?2 AND f.removed_at IS NULL \
             ORDER BY e.shared_at ASC",
        )?;
        let out: Vec<(String, String, Option<String>)> = stmt
            .query_map(rusqlite::params![key, fragment_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?
            .collect::<Result<_, _>>()?;
        Ok(out)
    }

    /// Creates a voice fragment. The audio already exists; this records that it does.
    ///
    /// The body starts empty and is filled in when a transcript arrives. A voice fragment
    /// with no transcript is complete, not broken.
    pub fn capture_voice(
        &self,
        audio_path: &str,
        duration_ms: i64,
        capture_origin: Option<&str>,
    ) -> Result<Fragment, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO fragments (id, body, captured_at, capture_method, capture_origin) \
             VALUES (?1, '', ?2, 'voice', ?3)",
            rusqlite::params![id, now, capture_origin],
        )?;
        conn.execute(
            "INSERT INTO voice_captures (fragment_id, audio_path, duration_ms, recorded_at) \
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, audio_path, duration_ms, now],
        )?;
        conn.execute(
            "INSERT INTO voice_transcripts (fragment_id, state) VALUES (?1, 'pending')",
            [&id],
        )?;

        conn.query_row(
            "SELECT id, body, captured_at, capture_method, capture_origin, corrected_at, \
             correction_count, legacy_edit_count FROM fragments WHERE id = ?1",
            [&id],
            |r| {
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
            },
        )
    }

    /// Records what the machine heard.
    ///
    /// The transcript is written to `voice_transcripts` AND used as the fragment's body, but
    /// only while the body is still empty: once a person has corrected the wording, a later
    /// re-transcription must not overwrite what they wrote.
    pub fn record_transcript(
        &self,
        fragment_id: &str,
        result: Result<(String, &str), String>,
    ) -> Result<(), rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();

        match result {
            Ok((text, model)) => {
                conn.execute(
                    "UPDATE voice_transcripts SET machine_transcript = ?1, state = 'ok', \
                     model = ?2, transcribed_at = ?3, failure = NULL WHERE fragment_id = ?4",
                    rusqlite::params![text, model, now, fragment_id],
                )?;

                let current: String = conn.query_row(
                    "SELECT body FROM fragments WHERE id = ?1",
                    [fragment_id],
                    |r| r.get(0),
                )?;
                let corrections: i64 = conn.query_row(
                    "SELECT correction_count FROM fragments WHERE id = ?1",
                    [fragment_id],
                    |r| r.get(0),
                )?;
                if current.trim().is_empty() && corrections == 0 {
                    conn.execute(
                        "UPDATE fragments SET body = ?1 WHERE id = ?2",
                        rusqlite::params![text, fragment_id],
                    )?;
                }
            }
            Err(reason) => {
                // "couldn't transcribe · the audio is safe" — the fragment is untouched.
                conn.execute(
                    "UPDATE voice_transcripts SET state = 'failed', transcribed_at = ?1, failure = ?2 \
                     WHERE fragment_id = ?3",
                    rusqlite::params![now, reason, fragment_id],
                )?;
            }
        }
        sync_fts(&conn, fragment_id)?;
        Ok(())
    }

    pub fn voice_for(&self, fragment_id: &str) -> Result<Option<VoiceCapture>, rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        conn.query_row(
            "SELECT v.fragment_id, v.audio_path, v.duration_ms, v.recorded_at, v.audio_missing, \
                    t.machine_transcript, COALESCE(t.state, 'pending'), t.transcribed_at, t.failure, \
                    f.body \
             FROM voice_captures v \
             LEFT JOIN voice_transcripts t ON t.fragment_id = v.fragment_id \
             JOIN fragments f ON f.id = v.fragment_id \
             WHERE v.fragment_id = ?1",
            [fragment_id],
            |r| {
                let machine: Option<String> = r.get(5)?;
                let body: String = r.get(9)?;
                Ok(VoiceCapture {
                    fragment_id: r.get(0)?,
                    audio_path: r.get(1)?,
                    duration_ms: r.get(2)?,
                    recorded_at: r.get(3)?,
                    audio_missing: r.get::<_, i64>(4)? != 0,
                    // Derived by comparison, so it cannot drift out of agreement with the body.
                    transcript_corrected: match &machine {
                        Some(m) => !m.is_empty() && m != &body,
                        None => false,
                    },
                    machine_transcript: machine,
                    transcript_state: r.get(6)?,
                    transcribed_at: r.get(7)?,
                    failure: r.get(8)?,
                })
            },
        )
        .optional()
    }

    /// Notes that a recording's audio could not be found. Does not remove the fragment.
    pub fn mark_audio_missing(&self, fragment_id: &str) -> Result<(), rusqlite::Error> {
        let conn = self.0.lock().unwrap();
        conn.execute(
            "UPDATE voice_captures SET audio_missing = 1 WHERE fragment_id = ?1",
            [fragment_id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn db() -> Db {
        Db::open(PathBuf::from(":memory:")).unwrap()
    }

    fn body_of(db: &Db, id: &str) -> String {
        let conn = db.0.lock().unwrap();
        conn.query_row("SELECT body FROM fragments WHERE id = ?1", [id], |r| r.get(0))
            .unwrap()
    }

    // ---------------------------------------------------------------- url identity

    #[test]
    fn the_same_article_through_a_newsletter_is_the_same_source() {
        let plain = "https://theatlantic.com/ideas/archive/2026/09/second-brain-apps/";
        let tracked =
            "https://www.theatlantic.com/ideas/archive/2026/09/second-brain-apps/?utm_source=newsletter&utm_campaign=q4";
        let anchored = "http://theatlantic.com/ideas/archive/2026/09/second-brain-apps/#section-3";
        assert_eq!(url_key(plain), url_key(tracked));
        assert_eq!(url_key(plain), url_key(anchored));
    }

    #[test]
    fn a_meaningful_query_parameter_still_distinguishes_pages() {
        // ?v= is the identity of a video, not a campaign.
        assert_ne!(
            url_key("https://example.com/watch?v=abc"),
            url_key("https://example.com/watch?v=def")
        );
        // Order of parameters is not meaning.
        assert_eq!(
            url_key("https://example.com/a?x=1&y=2"),
            url_key("https://example.com/a?y=2&x=1")
        );
    }

    #[test]
    fn url_key_never_replaces_the_url_that_was_received() {
        let db = db();
        let raw = "https://WWW.Example.com/A/b/?utm_source=x#frag";
        let f = db.capture_encounter(raw, "", None, None, None).unwrap();
        let e = db.encounter_for(&f.id).unwrap().unwrap();
        assert_eq!(e.url_raw, raw, "stored byte-for-byte, including case and tracking");
        assert_ne!(e.url_key, raw);
    }

    #[test]
    fn a_url_that_cannot_be_parsed_is_still_captured() {
        let db = db();
        for ugly in ["not a url at all", "http://", "://///", "mailto:someone@example.com", ""] {
            let f = db.capture_encounter(ugly, "some words", None, None, None).unwrap();
            let e = db.encounter_for(&f.id).unwrap().unwrap();
            assert_eq!(e.url_raw, ugly);
            // No domain is honest; a guessed one would not be.
            if !ugly.contains('.') {
                assert!(e.domain.is_none(), "{ugly} should not produce a domain");
            }
        }
    }

    // ---------------------------------------------------------------- provenance

    #[test]
    fn source_app_is_only_recorded_when_the_platform_supplied_one() {
        let db = db();
        let without = db
            .capture_encounter("https://example.com/a", "", None, None, None)
            .unwrap();
        assert!(db.encounter_for(&without.id).unwrap().unwrap().source_app.is_none());

        let with = db
            .capture_encounter("https://example.com/b", "", Some("com.apple.Safari"), None, None)
            .unwrap();
        assert_eq!(
            db.encounter_for(&with.id).unwrap().unwrap().source_app.as_deref(),
            Some("com.apple.Safari")
        );
    }

    #[test]
    fn capture_method_reflects_how_the_material_arrived() {
        let db = db();
        let pasted = db.capture_encounter("https://example.com/a", "", None, None, None).unwrap();
        let shared = db
            .capture_encounter("https://example.com/b", "", Some("com.apple.Safari"), None, None)
            .unwrap();
        assert_eq!(pasted.capture_method, "url");
        assert_eq!(shared.capture_method, "shared", "the OS told us it was a share");
    }

    // ---------------------------------------------------------------- enrichment

    #[test]
    fn capture_does_not_wait_for_or_depend_on_the_network() {
        let db = db();
        let f = db
            .capture_encounter("https://example.com/offline", "my words about it", None, None, None)
            .unwrap();
        let e = db.encounter_for(&f.id).unwrap().unwrap();

        assert_eq!(e.enrichment_state, "pending");
        assert!(e.title.is_none());
        assert!(e.url_canonical.is_none());
        // The domain is readable offline, so the fragment is never a bare string.
        assert_eq!(e.domain.as_deref(), Some("example.com"));
        assert_eq!(body_of(&db, &f.id), "my words about it");
    }

    #[test]
    fn a_failed_fetch_is_recorded_and_leaves_the_material_alone() {
        let db = db();
        let f = db.capture_encounter("https://example.com/a", "words", None, None, None).unwrap();
        let e = db.encounter_for(&f.id).unwrap().unwrap();

        db.record_enrichment(e.id, Err("dns failure".into())).unwrap();

        let after = db.encounter_for(&f.id).unwrap().unwrap();
        assert_eq!(after.enrichment_state, "failed");
        assert_eq!(after.failure.as_deref(), Some("dns failure"));
        assert!(after.title.is_none(), "a failure must not invent a title");
        assert_eq!(after.url_raw, "https://example.com/a");
        assert_eq!(body_of(&db, &f.id), "words");
    }

    #[test]
    fn a_title_that_arrives_later_becomes_searchable_without_altering_the_url() {
        let db = db();
        let f = db.capture_encounter("https://theatlantic.com/x", "", None, None, None).unwrap();
        let e = db.encounter_for(&f.id).unwrap().unwrap();

        db.record_enrichment(
            e.id,
            Ok((
                Some("Why Everyone Suddenly Wants a Second Brain".into()),
                Some("https://theatlantic.com/ideas/second-brain/".into()),
                Some("The Atlantic".into()),
            )),
        )
        .unwrap();

        let after = db.encounter_for(&f.id).unwrap().unwrap();
        assert_eq!(after.enrichment_state, "ok");
        assert_eq!(after.url_raw, "https://theatlantic.com/x", "the received URL is untouched");
        assert_eq!(
            after.url_canonical.as_deref(),
            Some("https://theatlantic.com/ideas/second-brain/")
        );

        let hits = db.find_fragments("Suddenly", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].matched_in, "source");
    }

    #[test]
    fn enrichment_can_change_its_mind_on_a_later_attempt() {
        let db = db();
        let f = db.capture_encounter("https://example.com/a", "", None, None, None).unwrap();
        let e = db.encounter_for(&f.id).unwrap().unwrap();

        db.record_enrichment(e.id, Err("offline".into())).unwrap();
        assert_eq!(db.encounter_for(&f.id).unwrap().unwrap().enrichment_state, "failed");

        db.record_enrichment(e.id, Ok((Some("A Title".into()), None, None))).unwrap();
        let after = db.encounter_for(&f.id).unwrap().unwrap();
        assert_eq!(after.enrichment_state, "ok");
        assert_eq!(after.title.as_deref(), Some("A Title"));
        assert!(after.failure.is_none(), "a success clears the old failure");
    }

    #[test]
    fn the_enrichment_queue_only_holds_what_has_never_been_tried() {
        let db = db();
        let a = db.capture_encounter("https://example.com/a", "", None, None, None).unwrap();
        let b = db.capture_encounter("https://example.com/b", "", None, None, None).unwrap();
        assert_eq!(db.encounters_awaiting_enrichment(10).unwrap().len(), 2);

        let ea = db.encounter_for(&a.id).unwrap().unwrap();
        db.record_enrichment(ea.id, Err("nope".into())).unwrap();
        let pending = db.encounters_awaiting_enrichment(10).unwrap();
        assert_eq!(pending.len(), 1, "a failure is a result, not an invitation to loop");
        assert_eq!(pending[0].1, "https://example.com/b");
        let _ = b;
    }

    // ---------------------------------------------------------------- repeat encounters

    #[test]
    fn meeting_the_same_source_again_is_durable_not_reconstructed() {
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
        let second = db
            .capture_encounter(
                "https://www.theatlantic.com/ideas/second-brain/?utm_source=newsletter",
                "",
                Some("com.apple.Safari"),
                None,
                None,
            )
            .unwrap();

        let met = db.same_source_encounters(&second.id).unwrap();
        assert_eq!(met.len(), 1);
        assert_eq!(met[0].0, first.id);

        // Both sides of the relationship agree, and the count is on the encounter itself.
        assert_eq!(db.encounter_for(&second.id).unwrap().unwrap().times_met, 2);
        assert_eq!(db.same_source_encounters(&first.id).unwrap().len(), 1);
    }

    #[test]
    fn a_source_met_once_reports_no_repeats() {
        let db = db();
        let only = db.capture_encounter("https://example.com/a", "", None, None, None).unwrap();
        assert!(db.same_source_encounters(&only.id).unwrap().is_empty());
        assert_eq!(db.encounter_for(&only.id).unwrap().unwrap().times_met, 1);
    }

    // ---------------------------------------------------------------- voice

    #[test]
    fn a_voice_fragment_exists_before_anything_is_transcribed() {
        let db = db();
        let f = db.capture_voice("/audio/a.wav", 42_000, Some("desktop")).unwrap();
        let v = db.voice_for(&f.id).unwrap().unwrap();

        assert_eq!(f.capture_method, "voice");
        assert_eq!(v.duration_ms, 42_000);
        assert_eq!(v.transcript_state, "pending");
        assert!(v.machine_transcript.is_none());
        assert_eq!(body_of(&db, &f.id), "", "no transcript yet, and that is a complete state");
    }

    #[test]
    fn a_failed_transcription_keeps_the_audio() {
        let db = db();
        let f = db.capture_voice("/audio/a.wav", 6_000, None).unwrap();
        db.record_transcript(&f.id, Err("no speech recognised".into())).unwrap();

        let v = db.voice_for(&f.id).unwrap().unwrap();
        assert_eq!(v.transcript_state, "failed");
        assert_eq!(v.failure.as_deref(), Some("no speech recognised"));
        assert_eq!(v.audio_path, "/audio/a.wav", "the audio is safe");
        assert!(!v.audio_missing);
    }

    #[test]
    fn a_transcript_fills_an_empty_body_and_is_searchable() {
        let db = db();
        let f = db.capture_voice("/audio/a.wav", 11_000, None).unwrap();
        db.record_transcript(&f.id, Ok(("a tag is a folder that is embarrassed".into(), "whisper")))
            .unwrap();

        assert_eq!(body_of(&db, &f.id), "a tag is a folder that is embarrassed");
        let hits = db.find_fragments("embarrassed", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].matched_in, "transcript");
    }

    /// The rule that matters most here.
    #[test]
    fn correcting_a_transcript_touches_neither_the_audio_nor_what_the_machine_heard() {
        let db = db();
        let f = db.capture_voice("/audio/a.wav", 11_000, None).unwrap();
        db.record_transcript(&f.id, Ok(("ref four four seven one".into(), "whisper")))
            .unwrap();

        db.correct_fragment(&f.id, "ref 4471").unwrap();

        let v = db.voice_for(&f.id).unwrap().unwrap();
        assert_eq!(v.audio_path, "/audio/a.wav", "audio is untouched by a wording change");
        assert_eq!(v.duration_ms, 11_000);
        assert_eq!(
            v.machine_transcript.as_deref(),
            Some("ref four four seven one"),
            "what the machine heard is a fact about the recording, not a draft"
        );
        assert!(v.transcript_corrected);
        assert_eq!(body_of(&db, &f.id), "ref 4471");

        // And the earlier wording survives as ordinary revision history.
        let history = db.fragment_history(&f.id).unwrap();
        assert_eq!(history[0].body, "ref four four seven one");
        assert_eq!(history[1].body, "ref 4471");
    }

    #[test]
    fn re_transcribing_never_overwrites_a_correction() {
        let db = db();
        let f = db.capture_voice("/audio/a.wav", 11_000, None).unwrap();
        db.record_transcript(&f.id, Ok(("teh wrong words".into(), "whisper"))).unwrap();
        db.correct_fragment(&f.id, "the right words").unwrap();

        // A second pass, perhaps with a better model.
        db.record_transcript(&f.id, Ok(("the wrong words again".into(), "whisper-v3")))
            .unwrap();

        assert_eq!(
            body_of(&db, &f.id),
            "the right words",
            "a machine must not talk over a person"
        );
        let v = db.voice_for(&f.id).unwrap().unwrap();
        assert_eq!(v.machine_transcript.as_deref(), Some("the wrong words again"));
    }

    #[test]
    fn missing_audio_is_recorded_without_losing_the_fragment() {
        let db = db();
        let f = db.capture_voice("/audio/gone.wav", 3_000, None).unwrap();
        db.record_transcript(&f.id, Ok(("something said".into(), "whisper"))).unwrap();

        db.mark_audio_missing(&f.id).unwrap();

        let v = db.voice_for(&f.id).unwrap().unwrap();
        assert!(v.audio_missing);
        assert_eq!(body_of(&db, &f.id), "something said", "the words survive the audio");
        assert_eq!(db.find_fragments("something", 10).unwrap().len(), 1);
    }

    #[test]
    fn a_very_long_transcript_is_stored_and_searchable_whole() {
        let db = db();
        let long = "so the thing about the onboarding is ".repeat(400);
        let f = db.capture_voice("/audio/long.wav", 1_800_000, None).unwrap();
        db.record_transcript(&f.id, Ok((long.clone(), "whisper"))).unwrap();

        assert_eq!(body_of(&db, &f.id).len(), long.len());
        assert_eq!(db.find_fragments("onboarding", 10).unwrap().len(), 1);
    }

    #[test]
    fn mixed_text_and_url_keeps_the_two_kinds_of_material_apart() {
        let db = db();
        let f = db
            .capture_encounter(
                "https://maggieappleton.com/garden",
                "still don't buy it but the tending metaphor is fine",
                Some("com.apple.Safari"),
                Some("A Brief History & Ethos of the Digital Garden"),
                None,
            )
            .unwrap();

        let e = db.encounter_for(&f.id).unwrap().unwrap();
        assert_eq!(body_of(&db, &f.id), "still don't buy it but the tending metaphor is fine");
        assert_eq!(
            e.selected_text.as_deref(),
            Some("A Brief History & Ethos of the Digital Garden"),
            "what the source said stays distinct from what the person said"
        );

        // Both are findable, and Find can say which answered.
        assert_eq!(db.find_fragments("tending", 10).unwrap()[0].matched_in, "body");
        assert_eq!(db.find_fragments("Ethos", 10).unwrap()[0].matched_in, "source");
    }
}
