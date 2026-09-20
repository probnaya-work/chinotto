//! Tauri commands for the Record.
//!
//! Thin by design: these translate arguments and errors and nothing else. Every rule about
//! what capture, Continue and Correct mean lives in `db::record`, so the guarantees hold
//! whether a call arrives from the window, the tray, or a test.

use crate::db::bridge::WordingConflict;
use crate::db::record::{Fragment, FindHit, HeldFragment, LineMoment, Revision};
use crate::db::material::{Encounter, PageMaterials, VoiceCapture};
use crate::db::meaning::Guess;
use crate::db::returns::Return;
use crate::db::Db;

/// Mirrors a fragment into the legacy `entries` row the sync protocol reads.
///
/// Called after the write, never during it: the db methods hold the connection lock, so
/// bridging inside them would deadlock. A mirror failure is logged and swallowed — the
/// Record is canonical, and a capture must not fail because a compatibility shim did.
fn bridge(db: &tauri::State<Db>, fragment_id: &str) {
    if let Err(e) = db.mirror_fragment_to_entry(fragment_id) {
        eprintln!("[bridge] mirror failed for {fragment_id}: {e}");
    }
}

/// Capture cannot fail for a reason the person has to care about, so the only rejection
/// here is material with nothing in it.
#[tauri::command]
pub fn capture_fragment(
    db: tauri::State<Db>,
    body: String,
    capture_method: Option<String>,
    capture_origin: Option<String>,
) -> Result<Fragment, String> {
    // Trailing whitespace is trimmed, interior shape is not: a fragment may be several
    // lines and that is the person's business.
    let body = body.trim_end();
    if body.trim().is_empty() {
        return Err("nothing to leave".to_string());
    }
    let fragment = db
        .capture_fragment(
            body,
            capture_method.as_deref().unwrap_or("typed"),
            capture_origin.as_deref(),
        )
        .map_err(|e| e.to_string())?;
    bridge(&db, &fragment.id);
    Ok(fragment)
}

#[tauri::command]
pub fn continue_fragment(
    db: tauri::State<Db>,
    continues_id: String,
    body: String,
    capture_method: Option<String>,
    capture_origin: Option<String>,
) -> Result<Fragment, String> {
    let body = body.trim_end();
    if body.trim().is_empty() {
        return Err("nothing to leave".to_string());
    }
    let fragment = db
        .continue_fragment(
            &continues_id,
            body,
            capture_method.as_deref().unwrap_or("typed"),
            capture_origin.as_deref(),
            false,
        )
        .map_err(|e| e.to_string())?;
    // A continuation is a new dated moment, so it mirrors as its own entry. Appending it to
    // the earlier row would manufacture v1's in-text continuation and misdate the words.
    bridge(&db, &fragment.id);
    Ok(fragment)
}

/// "yes, that continues yesterday's note" — the capture already happened and stays valid.
#[tauri::command]
pub fn link_continuation(
    db: tauri::State<Db>,
    fragment_id: String,
    continues_id: String,
) -> Result<(), String> {
    db.link_continuation(&fragment_id, &continues_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn correct_fragment(
    db: tauri::State<Db>,
    id: String,
    body: String,
) -> Result<crate::db::record::Fragment, String> {
    let body = body.trim_end();
    if body.trim().is_empty() {
        return Err("a correction cannot empty a moment".to_string());
    }
    db.correct_fragment(&id, body).map_err(|e| e.to_string())?;
    // The legacy row carries the current wording; the wording it replaced stays in
    // fragment_revisions, which the legacy model has no way to express.
    bridge(&db, &id);
    // Returns the fragment rather than a count so the caller can push it without having to
    // reconstruct its capture time — an empty created_at would corrupt the remote document.
    db.fragment(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "fragment vanished during correction".to_string())
}

#[tauri::command]
pub fn fragment_history(db: tauri::State<Db>, id: String) -> Result<Vec<Revision>, String> {
    db.fragment_history(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn line_for(db: tauri::State<Db>, id: String) -> Result<Vec<LineMoment>, String> {
    db.line_for(&id).map_err(|e| e.to_string())
}

/// Returns `(held_count, taken)`. `taken = false` means the bound was reached and nothing
/// was changed — the caller says so rather than silently doing nothing.
#[tauri::command]
pub fn hold_fragment(db: tauri::State<Db>, id: String) -> Result<(i64, bool), String> {
    db.hold_fragment(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn max_held() -> i64 {
    crate::db::record::MAX_HELD
}

#[tauri::command]
pub fn release_fragment(db: tauri::State<Db>, id: String) -> Result<(), String> {
    db.release_fragment(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn held_fragments(db: tauri::State<Db>) -> Result<Vec<HeldFragment>, String> {
    db.held_fragments().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn recent_fragments(db: tauri::State<Db>, limit: Option<i64>) -> Result<Vec<Fragment>, String> {
    db.recent_fragments(limit.unwrap_or(50).clamp(1, 500))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn fragments_before(
    db: tauri::State<Db>,
    before: Option<String>,
    limit: Option<i64>,
) -> Result<Vec<Fragment>, String> {
    db.fragments_before(before.as_deref(), limit.unwrap_or(100).clamp(1, 500))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn fragments_between(
    db: tauri::State<Db>,
    from: String,
    to: String,
    limit: Option<i64>,
) -> Result<Vec<Fragment>, String> {
    db.fragments_between(&from, &to, limit.unwrap_or(500).clamp(1, 2000))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn find_fragments(
    db: tauri::State<Db>,
    query: String,
    limit: Option<i64>,
) -> Result<Vec<FindHit>, String> {
    db.find_fragments(&query, limit.unwrap_or(200).clamp(1, 1000))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn month_density(db: tauri::State<Db>, year: i32) -> Result<Vec<i64>, String> {
    db.month_density(year).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn record_span(db: tauri::State<Db>) -> Result<Option<(String, String)>, String> {
    db.record_span().map_err(|e| e.to_string())
}

/// At most one Return, or none. None is the ordinary answer: silence is valid.
#[tauri::command]
pub fn select_return(db: tauri::State<Db>) -> Result<Option<Return>, String> {
    let now = chrono::Utc::now().to_rfc3339();
    db.select_return(&now).map_err(|e| e.to_string())
}

/// 'opened' | 'continued' | 'let_go' | 'expired'. Letting go is an outcome, not a delete.
#[tauri::command]
pub fn record_return_outcome(db: tauri::State<Db>, id: i64, outcome: String) -> Result<(), String> {
    db.record_return_outcome(id, &outcome).map_err(|e| e.to_string())
}

/// Guesses. A separate command from `find_fragments` on purpose: exact retrieval must never
/// wait on the model, and a caller has to opt in to asking for inference.
#[tauri::command]
pub fn find_by_meaning(
    db: tauri::State<Db>,
    query: String,
    exclude: Option<Vec<String>>,
    limit: Option<usize>,
) -> Result<Vec<Guess>, String> {
    db.find_by_meaning(&query, &exclude.unwrap_or_default(), limit.unwrap_or(5).min(20))
}

/// "not this".
#[tauri::command]
pub fn reject_guess(db: tauri::State<Db>, fragment_id: String, related_id: String) -> Result<(), String> {
    db.reject_guess(&fragment_id, &related_id).map_err(|e| e.to_string())
}

/// Embeds a slice of whatever still needs it. Called opportunistically, never blocking.
#[tauri::command]
pub fn embed_pending(db: tauri::State<Db>, limit: Option<i64>) -> Result<usize, String> {
    db.embed_pending(limit.unwrap_or(32).clamp(1, 512))
}

#[tauri::command]
pub fn pending_embedding_count(db: tauri::State<Db>) -> Result<i64, String> {
    db.pending_embedding_count().map_err(|e| e.to_string())
}

// ------------------------------------------------------------------ external material

/// Captures a URL. Local and immediate; no network call happens on this path, so sharing a
/// link behaves identically offline.
#[tauri::command]
pub fn capture_encounter(
    db: tauri::State<Db>,
    url_raw: String,
    body: Option<String>,
    source_app: Option<String>,
    selected_text: Option<String>,
    capture_origin: Option<String>,
) -> Result<crate::db::record::Fragment, String> {
    if url_raw.trim().is_empty() {
        return Err("nothing to leave".to_string());
    }
    db.capture_encounter(
        url_raw.trim(),
        body.as_deref().unwrap_or("").trim_end(),
        // Only what the platform actually handed us. The frontend must not synthesise this.
        source_app.as_deref().filter(|s| !s.trim().is_empty()),
        selected_text.as_deref().filter(|s| !s.trim().is_empty()),
        capture_origin.as_deref(),
    )
    .map_err(|e| e.to_string())
    .inspect(|f| bridge(&db, &f.id))
}

#[tauri::command]
pub fn encounter_for(db: tauri::State<Db>, fragment_id: String) -> Result<Option<Encounter>, String> {
    db.encounter_for(&fragment_id).map_err(|e| e.to_string())
}

/// Every other time this source was met. Reads the key stored at capture.
#[tauri::command]
pub fn same_source_encounters(
    db: tauri::State<Db>,
    fragment_id: String,
) -> Result<Vec<(String, String, Option<String>)>, String> {
    db.same_source_encounters(&fragment_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn encounters_awaiting_enrichment(
    db: tauri::State<Db>,
    limit: Option<i64>,
) -> Result<Vec<(i64, String)>, String> {
    db.encounters_awaiting_enrichment(limit.unwrap_or(8).clamp(1, 50))
        .map_err(|e| e.to_string())
}

/// Records the outcome of a metadata fetch. Failure is a result, not an error to retry forever.
#[tauri::command]
pub fn record_enrichment(
    db: tauri::State<Db>,
    encounter_id: i64,
    title: Option<String>,
    url_canonical: Option<String>,
    site_name: Option<String>,
    failure: Option<String>,
) -> Result<(), String> {
    let result = match failure {
        Some(reason) => Err(reason),
        None => Ok((title, url_canonical, site_name)),
    };
    db.record_enrichment(encounter_id, result).map_err(|e| e.to_string())
}

// ------------------------------------------------------------------ voice

/// Records that a recording exists. The audio is the fragment; a transcript may follow.
#[tauri::command]
pub fn capture_voice(
    db: tauri::State<Db>,
    audio_path: String,
    duration_ms: i64,
    capture_origin: Option<String>,
) -> Result<crate::db::record::Fragment, String> {
    db.capture_voice(&audio_path, duration_ms.max(0), capture_origin.as_deref())
        .map_err(|e| e.to_string())
}

/// What the machine heard, or why it could not hear anything.
#[tauri::command]
pub fn record_transcript(
    db: tauri::State<Db>,
    fragment_id: String,
    transcript: Option<String>,
    model: Option<String>,
    failure: Option<String>,
) -> Result<(), String> {
    let result = match (transcript, failure) {
        (_, Some(reason)) => Err(reason),
        (Some(text), None) => Ok((text, model.as_deref().unwrap_or("unknown"))),
        (None, None) => Err("no transcript produced".to_string()),
    };
    db.record_transcript(&fragment_id, result)
        .map_err(|e| e.to_string())?;
    // A voice fragment has no text to mirror until a transcript exists. The audio itself has
    // no legacy representation and stays on this device.
    bridge(&db, &fragment_id);
    Ok(())
}

#[tauri::command]
pub fn voice_for(db: tauri::State<Db>, fragment_id: String) -> Result<Option<VoiceCapture>, String> {
    db.voice_for(&fragment_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn mark_audio_missing(db: tauri::State<Db>, fragment_id: String) -> Result<(), String> {
    db.mark_audio_missing(&fragment_id).map_err(|e| e.to_string())
}

/// Material for a page of fragments, in one pass.
#[tauri::command]
pub fn materials_for(
    db: tauri::State<Db>,
    ids: Vec<String>,
) -> Result<PageMaterials, String> {
    db.materials_for(&ids).map_err(|e| e.to_string())
}

// ------------------------------------------------------------------ legacy bridge

/// Removes a fragment: soft in the Record, hard on the legacy row, tombstoned for sync.
#[tauri::command]
pub fn remove_fragment(db: tauri::State<Db>, id: String) -> Result<(), String> {
    db.remove_fragment(&id).map_err(|e| e.to_string())
}

/// Undo for a removal made on this device.
#[tauri::command]
pub fn restore_fragment(db: tauri::State<Db>, id: String) -> Result<(), String> {
    db.restore_fragment(&id).map_err(|e| e.to_string())
}

/// Projects any legacy entries that have arrived over sync into the Record.
#[tauri::command]
pub fn project_entries_into_record(db: tauri::State<Db>) -> Result<usize, String> {
    db.project_entries_into_record().map_err(|e| e.to_string())
}

/// Soft-removes fragments whose legacy rows were deleted by another device.
#[tauri::command]
pub fn absorb_remote_deletes(db: tauri::State<Db>, ids: Vec<String>) -> Result<usize, String> {
    db.absorb_remote_deletes(&ids).map_err(|e| e.to_string())
}

/// How many fragments the bridge still owes the legacy table, without mirroring them.
///
/// Read-only on purpose: the sync surface states a number, and a status read that also
/// performs the work it is reporting on can never show a steady figure.
#[tauri::command]
pub fn fragments_awaiting_mirror(db: tauri::State<Db>, limit: Option<i64>) -> Result<usize, String> {
    db.fragments_awaiting_mirror(limit.unwrap_or(500).clamp(1, 5000))
        .map(|ids| ids.len())
        .map_err(|e| e.to_string())
}

/// Fragments the bridge still owes the legacy table, e.g. after an upgrade or time offline.
#[tauri::command]
pub fn mirror_pending_fragments(db: tauri::State<Db>, limit: Option<i64>) -> Result<usize, String> {
    let ids = db
        .fragments_awaiting_mirror(limit.unwrap_or(200).clamp(1, 2000))
        .map_err(|e| e.to_string())?;
    let mut done = 0;
    for id in ids {
        if db.mirror_fragment_to_entry(&id).map_err(|e| e.to_string())? {
            done += 1;
        }
    }
    Ok(done)
}


/// This install's own id and name, for the cloud's device list.
#[tauri::command]
pub fn this_device(db: tauri::State<Db>) -> Result<(String, String), String> {
    db.this_device().map_err(|e| e.to_string())
}

/// Moments that were worded in two places and have not been settled yet.
#[tauri::command]
pub fn open_wording_conflicts(db: tauri::State<Db>) -> Result<Vec<WordingConflict>, String> {
    db.open_wording_conflicts().map_err(|e| e.to_string())
}

/// Chooses which wording shows. The other stays under the moment as earlier wording.
#[tauri::command]
pub fn resolve_wording_conflict(
    db: tauri::State<Db>,
    fragment_id: String,
    shows: String,
) -> Result<(), String> {
    if shows != "local" && shows != "remote" {
        return Err("a wording is either the local one or the remote one".into());
    }
    db.resolve_wording_conflict(&fragment_id, &shows)
        .map_err(|e| e.to_string())
}
