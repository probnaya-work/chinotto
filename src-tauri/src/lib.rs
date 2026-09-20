mod db;
mod record_commands;
mod embeddings;
mod keywords;
mod oauth_dev_bridge;
mod recall;
mod themes;

#[cfg(test)]
mod thought_trail;

#[cfg(target_os = "macos")]
mod native_apple_sign_in;

#[cfg(target_os = "macos")]
mod speech;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod tray_capture;

use base64::Engine;
use chrono::TimeZone;
use db::{Db, SpaceFilter};
use keywords::{
    capture_continuation_max_days, capture_continuation_min_overlap, extract_keywords,
    keyword_overlap, shared_keywords, thought_trail_candidates, thought_trail_max_related,
    thought_trail_min_overlap, thought_trail_similarity,
};
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc;
use rand::seq::SliceRandom;
use std::sync::Arc;
use tauri::Emitter;
use tauri::Manager;

fn resolve_space_filter(db: &Db, raw: Option<String>) -> Result<SpaceFilter, String> {
    match raw.as_deref() {
        None | Some("") => Ok(SpaceFilter::All),
        Some("inbox") => Ok(SpaceFilter::Inbox),
        Some(id) => {
            if db.space_id_valid(id).map_err(|e| e.to_string())? {
                Ok(SpaceFilter::Space(id.to_string()))
            } else {
                Err(format!("unknown space: {}", id))
            }
        }
    }
}

fn parse_created_at(iso: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(iso)
        .ok()
        .map(|dt| dt.with_timezone(&chrono::Utc))
}

fn format_ago(
    created: chrono::DateTime<chrono::Utc>,
    now: chrono::DateTime<chrono::Utc>,
) -> String {
    let d = (now - created).num_days();
    if d >= 365 {
        let y = d / 365;
        format!("{} year{} ago", y, if y == 1 { "" } else { "s" })
    } else if d >= 30 {
        let m = d / 30;
        format!("{} month{} ago", m, if m == 1 { "" } else { "s" })
    } else if d >= 7 {
        let w = d / 7;
        format!("{} week{} ago", w, if w == 1 { "" } else { "s" })
    } else if d >= 1 {
        format!("{} day{} ago", d, if d == 1 { "" } else { "s" })
    } else {
        "earlier today".to_string()
    }
}

/// Memory-style reason for temporal recall (24h / 7d / 30d anchors).
fn temporal_reason_anchor(anchor: &str) -> &'static str {
    match anchor {
        "24h" => "From yesterday",
        "7d" => "From last week",
        "30d" => "From about a month ago",
        _ => "From a while ago",
    }
}

/// Simple importance score from existing signals: pinned, edited, opened.
/// Used only as a small ranking boost in recall (resurface, thought trail).
/// Formula: pin_weight (1 if pinned) + edit_weight (capped) + open_weight (capped).
const IMPORTANCE_PIN: f64 = 1.0;
const IMPORTANCE_EDIT_FACTOR: f64 = 0.5;
const IMPORTANCE_EDIT_CAP: f64 = 2.0;
const IMPORTANCE_OPEN_FACTOR: f64 = 0.2;
const IMPORTANCE_OPEN_CAP: f64 = 1.5;

fn importance_score(entry: &db::EntryRow, pinned_ids: &std::collections::HashSet<String>) -> f64 {
    let pin = if pinned_ids.contains(&entry.id) {
        IMPORTANCE_PIN
    } else {
        0.0
    };
    let edit = (entry.edit_count as f64 * IMPORTANCE_EDIT_FACTOR).min(IMPORTANCE_EDIT_CAP);
    let open = (entry.open_count as f64 * IMPORTANCE_OPEN_FACTOR).min(IMPORTANCE_OPEN_CAP);
    pin + edit + open
}

/// Boost factor for recall ranking: 1.0 + small weight * importance (max ~1.2).
const IMPORTANCE_BOOST_WEIGHT: f64 = 0.08;

fn importance_boost(importance: f64) -> f64 {
    1.0 + IMPORTANCE_BOOST_WEIGHT * importance
}

/// Native Sign in with Apple → Firebase `OAuthProvider.credentialFromJSON` (`idToken` + unhashed `nonce`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeAppleSignInResult {
    pub id_token: String,
    pub raw_nonce: String,
}

#[tauri::command]
async fn native_apple_sign_in(app: tauri::AppHandle) -> Result<NativeAppleSignInResult, String> {
    #[cfg(target_os = "macos")]
    {
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || native_apple_sign_in::run(&app))
            .await
            .map_err(|e| e.to_string())?
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Err("Native Sign in with Apple is only available on macOS.".to_string())
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct FirestoreEntryIn {
    id: String,
    text: String,
    created_at: String,
}

/// Ingest remote entries (Firestore pull). Idempotent per mobile sync.md: existing `id` skipped.
#[tauri::command(async)]
fn ingest_firestore_entries(
    db: tauri::State<Db>,
    entries: Vec<FirestoreEntryIn>,
) -> Result<u32, String> {
    let batch: Vec<(String, String, String)> = entries
        .into_iter()
        .map(|e| (e.id, e.text, e.created_at))
        .collect();
    let inserted = db
        .ingest_firestore_entries(&batch)
        .map_err(|e| e.to_string())?;
    // Anything that just arrived from mobile becomes a fragment, or the Record would
    // silently diverge from the table sync writes into.
    if let Err(e) = db.project_entries_into_record() {
        eprintln!("[bridge] projecting ingested entries failed: {e}");
    }
    Ok(inserted)
}

#[tauri::command(async)]
fn enqueue_sync_tombstone(db: tauri::State<Db>, entry_id: String) -> Result<(), String> {
    db.enqueue_sync_tombstone(&entry_id).map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn list_sync_tombstone_outbox(db: tauri::State<Db>) -> Result<Vec<String>, String> {
    db.list_sync_tombstone_outbox().map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn list_due_sync_tombstone_outbox(
    db: tauri::State<Db>,
    min_age_secs: i64,
) -> Result<Vec<String>, String> {
    db.list_due_sync_tombstone_outbox(min_age_secs)
        .map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn remove_sync_tombstone_outbox(db: tauri::State<Db>, entry_id: String) -> Result<(), String> {
    db.remove_sync_tombstone_outbox(&entry_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn clear_sync_tombstone_outbox_all(db: tauri::State<Db>) -> Result<(), String> {
    db.clear_sync_tombstone_outbox_all().map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn clear_firestore_ingest_suppression(db: tauri::State<Db>, entry_id: String) -> Result<(), String> {
    db.clear_firestore_ingest_suppression(&entry_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn delete_local_entries_for_sync(
    db: tauri::State<Db>,
    entry_ids: Vec<String>,
) -> Result<u32, String> {
    let deleted = db
        .delete_local_entries_for_sync(&entry_ids)
        .map_err(|e| e.to_string())?;
    // A delete made on another device reaches the Record as a soft removal: the material
    // stays here, it just stops being present.
    if let Err(e) = db.absorb_remote_deletes(&entry_ids) {
        eprintln!("[bridge] absorbing remote deletes failed: {e}");
    }
    Ok(deleted)
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteEntryThemeIn {
    theme_id: String,
    confidence: f64,
    source: String,
    locked: bool,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApplyRemoteEntryThemeIn {
    entry_id: String,
    theme: Option<RemoteEntryThemeIn>,
}

#[tauri::command(async)]
fn apply_remote_entry_theme(
    db: tauri::State<Db>,
    input: ApplyRemoteEntryThemeIn,
) -> Result<bool, String> {
    let theme = input.theme.map(|t| crate::db::EntryThemeRow {
        theme_id: t.theme_id,
        confidence: t.confidence,
        source: t.source,
        locked: t.locked,
    });
    db.apply_remote_entry_theme(&input.entry_id, theme.as_ref())
        .map_err(|e| e.to_string())
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteUserThemeIn {
    id: String,
    label: String,
    sort_order: i32,
}

#[tauri::command(async)]
fn ingest_remote_user_themes(
    db: tauri::State<Db>,
    rows: Vec<RemoteUserThemeIn>,
) -> Result<u32, String> {
    let batch: Vec<(String, String, i32)> = rows
        .into_iter()
        .map(|r| (r.id, r.label, r.sort_order))
        .collect();
    db.ingest_remote_user_themes(&batch)
        .map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn apply_remote_user_theme_tombstones(
    db: tauri::State<Db>,
    theme_ids: Vec<String>,
) -> Result<u32, String> {
    db.apply_remote_user_theme_tombstones(&theme_ids)
        .map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct UserThemeOutboxRowOut {
    theme_id: String,
    op: String,
    label: Option<String>,
    sort_order: Option<i32>,
}

#[tauri::command(async)]
fn list_sync_user_theme_outbox(db: tauri::State<Db>) -> Result<Vec<UserThemeOutboxRowOut>, String> {
    db.list_sync_user_theme_outbox()
        .map(|rows| {
            rows.into_iter()
                .map(|(theme_id, op, label, sort_order)| UserThemeOutboxRowOut {
                    theme_id,
                    op,
                    label,
                    sort_order,
                })
                .collect()
        })
        .map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn remove_sync_user_theme_outbox(db: tauri::State<Db>, theme_id: String) -> Result<(), String> {
    db.remove_sync_user_theme_outbox(&theme_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn clear_sync_user_theme_outbox_all(db: tauri::State<Db>) -> Result<(), String> {
    db.clear_sync_user_theme_outbox_all()
        .map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn clear_user_theme_ingest_suppression(db: tauri::State<Db>, theme_id: String) -> Result<(), String> {
    db.clear_user_theme_ingest_suppression(&theme_id)
        .map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn enqueue_all_local_user_themes_for_sync(db: tauri::State<Db>) -> Result<(), String> {
    db.enqueue_all_local_user_themes_for_sync()
        .map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn list_entry_ids_with_themes(db: tauri::State<Db>) -> Result<Vec<String>, String> {
    db.list_entry_ids_with_themes()
        .map_err(|e| e.to_string())
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateEntryIn {
    text: String,
    #[serde(default)]
    space_id: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RestoreEntryIn {
    id: String,
    text: String,
    created_at: String,
    #[serde(default)]
    space_id: Option<String>,
}

fn validated_space_for_write(db: &Db, space_id: &Option<String>) -> Result<Option<String>, String> {
    match space_id.as_deref() {
        None | Some("") => Ok(None),
        Some(id) => {
            if db.space_id_valid(id).map_err(|e| e.to_string())? {
                Ok(Some(id.to_string()))
            } else {
                Err(format!("unknown space: {}", id))
            }
        }
    }
}

#[tauri::command(async)]
fn create_entry(db: tauri::State<Db>, input: CreateEntryIn) -> Result<String, String> {
    let trimmed = input.text.trim();
    if trimmed.is_empty() {
        return Err("entry text cannot be empty".to_string());
    }
    let sid = validated_space_for_write(&db, &input.space_id)?;
    let id = uuid::Uuid::new_v4().to_string();
    let created_at = chrono::Utc::now().to_rfc3339();
    db.create_entry(&id, trimmed, &created_at, sid.as_deref())
        .map_err(|e| e.to_string())?;
    Ok(id)
}

#[tauri::command(async)]
fn restore_entry(db: tauri::State<Db>, input: RestoreEntryIn) -> Result<String, String> {
    let trimmed = input.text.trim();
    if trimmed.is_empty() {
        return Err("entry text cannot be empty".to_string());
    }
    let sid = validated_space_for_write(&db, &input.space_id)?;
    match db.create_entry(&input.id, trimmed, &input.created_at, sid.as_deref()) {
        Ok(()) => Ok(input.id),
        Err(e) => {
            if e.to_string().contains("UNIQUE constraint") {
                let new_id = uuid::Uuid::new_v4().to_string();
                db.create_entry(&new_id, trimmed, &input.created_at, sid.as_deref())
                    .map_err(|e| e.to_string())?;
                Ok(new_id)
            } else {
                Err(e.to_string())
            }
        }
    }
}

#[tauri::command(async)]
fn generate_embedding(app: tauri::AppHandle, entry_id: String) -> Result<(), String> {
    let text = {
        let db = app.state::<Db>();
        let entry = db
            .get_entry_by_id(&entry_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "entry not found".to_string())?;
        entry.text
    };
    std::thread::spawn(move || {
        let db = app.state::<Db>();
        if let Err(e) = store_embedding_for_entry(&db, &entry_id, &text) {
            log::warn!("embedding refresh failed for {}: {}", entry_id, e);
        }
    });
    Ok(())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct EntryThemeOut {
    theme_id: String,
    confidence: f64,
    source: String,
    locked: bool,
}

fn classify_entry_theme_for_entry(db: &Db, entry_id: &str) -> Result<(), String> {
    let entry = db
        .get_entry_by_id(entry_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "entry not found".to_string())?;
    if db
        .entry_theme_locked(entry_id)
        .map_err(|e| e.to_string())?
    {
        return Ok(());
    }
    if let Some(classification) = themes::classify_entry_text(&entry.text) {
        db.upsert_entry_theme(
            entry_id,
            &classification.theme_id,
            classification.confidence,
            classification.source,
        )
        .map_err(|e| e.to_string())?;
    } else {
        db.clear_entry_theme_if_unlocked(entry_id)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command(async)]
fn classify_entry_theme(db: tauri::State<Db>, entry_id: String) -> Result<(), String> {
    classify_entry_theme_for_entry(&db, &entry_id)
}

#[tauri::command(async)]
fn get_entry_theme(
    db: tauri::State<Db>,
    entry_id: String,
) -> Result<Option<EntryThemeOut>, String> {
    let row = db.get_entry_theme(&entry_id).map_err(|e| e.to_string())?;
    Ok(row.map(|r| EntryThemeOut {
        theme_id: r.theme_id,
        confidence: r.confidence,
        source: r.source,
        locked: r.locked,
    }))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetEntryThemeIn {
    entry_id: String,
    theme_id: Option<String>,
    locked: bool,
}

#[tauri::command(async)]
fn set_entry_theme(db: tauri::State<Db>, input: SetEntryThemeIn) -> Result<(), String> {
    if let Some(theme_id) = input.theme_id.as_deref() {
        if !db.theme_id_valid(theme_id).map_err(|e| e.to_string())? {
            return Err("invalid theme".into());
        }
    }
    db.set_entry_theme(
        &input.entry_id,
        input.theme_id.as_deref(),
        input.locked,
    )
    .map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct UserThemeOut {
    id: String,
    label: String,
    sort_order: i32,
}

fn user_theme_out(row: crate::db::UserThemeRow) -> UserThemeOut {
    UserThemeOut {
        id: row.id,
        label: row.label,
        sort_order: row.sort_order,
    }
}

#[tauri::command(async)]
fn list_user_themes(db: tauri::State<Db>) -> Result<Vec<UserThemeOut>, String> {
    let rows = db.list_user_themes().map_err(|e| e.to_string())?;
    Ok(rows.into_iter().map(user_theme_out).collect())
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateUserThemeIn {
    label: String,
}

#[tauri::command(async)]
fn create_user_theme(
    db: tauri::State<Db>,
    input: CreateUserThemeIn,
) -> Result<UserThemeOut, String> {
    let row = db
        .create_user_theme(&input.label)
        .map_err(|e| e.to_string())?;
    Ok(user_theme_out(row))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateUserThemeIn {
    id: String,
    label: String,
}

#[tauri::command(async)]
fn update_user_theme(
    db: tauri::State<Db>,
    input: UpdateUserThemeIn,
) -> Result<UserThemeOut, String> {
    let row = db
        .update_user_theme(&input.id, &input.label)
        .map_err(|e| e.to_string())?;
    Ok(user_theme_out(row))
}

#[tauri::command(async)]
fn delete_user_theme(db: tauri::State<Db>, id: String) -> Result<(), String> {
    db.delete_user_theme(&id).map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ThemeCountOut {
    theme_id: String,
    count: i64,
}

#[tauri::command(async)]
fn list_theme_counts(db: tauri::State<Db>) -> Result<Vec<ThemeCountOut>, String> {
    let rows = db
        .list_theme_counts(crate::db::THEME_RECALL_MIN_CONFIDENCE)
        .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|(theme_id, count)| ThemeCountOut { theme_id, count })
        .collect())
}

#[tauri::command(async)]
fn list_theme_counts_recent(
    db: tauri::State<Db>,
    days: Option<u32>,
) -> Result<Vec<ThemeCountOut>, String> {
    let days = days.unwrap_or(7).max(1);
    let cutoff = chrono::Utc::now() - chrono::Duration::days(days as i64);
    let since = cutoff.to_rfc3339();
    let rows = db
        .list_theme_counts_since(crate::db::THEME_RECALL_MIN_CONFIDENCE, &since)
        .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|(theme_id, count)| ThemeCountOut { theme_id, count })
        .collect())
}

/// Minimum cosine similarity for an entry to appear in "Related thoughts".
/// Without this, the top-N by score included weak matches (e.g. entry about Tauri → movie title).
/// Embedding similarity has no cutoff, so unrelated text can still score 0.2–0.4; we require ~0.5+.
const MIN_RELATED_SIMILARITY: f32 = 0.5;

/// Recent entries scanned for missing embeddings (aligned with thought-trail candidate window).
const RELATED_EMBED_BACKFILL_SCAN: usize = 250;
/// Cap embeddings computed per related lookup so first detail open stays responsive.
const RELATED_EMBED_BACKFILL_PER_CALL: usize = 32;

fn store_embedding_for_entry(db: &Db, entry_id: &str, text: &str) -> Result<(), String> {
    let vec = embeddings::embed_text(text).map_err(|e| e.to_string())?;
    db.insert_embedding(entry_id, &vec)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Index the current entry and recent rows missing embeddings (frontend `generate_embedding` is fire-and-forget and can fail silently).
fn ensure_embeddings_for_related_search(db: &Db, entry_id: &str) -> Result<(), String> {
    let current = db
        .get_entry_by_id(entry_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "entry not found".to_string())?;
    if db
        .get_embedding(entry_id)
        .map_err(|e| e.to_string())?
        .is_none()
    {
        store_embedding_for_entry(db, entry_id, &current.text)?;
    }

    let all = db.list_entries().map_err(|e| e.to_string())?;
    let mut embedded_this_call = 0usize;
    for row in all.into_iter().take(RELATED_EMBED_BACKFILL_SCAN) {
        if row.id == entry_id {
            continue;
        }
        if embedded_this_call >= RELATED_EMBED_BACKFILL_PER_CALL {
            break;
        }
        if db
            .get_embedding(&row.id)
            .map_err(|e| e.to_string())?
            .is_some()
        {
            continue;
        }
        match store_embedding_for_entry(db, &row.id, &row.text) {
            Ok(()) => embedded_this_call += 1,
            Err(e) => log::warn!("embedding backfill failed for {}: {}", row.id, e),
        }
    }
    Ok(())
}

/// Filter (id, similarity) pairs by min_sim, sort by score descending, take top `limit` ids.
/// Used by find_similar_entries so threshold is applied before sort/limit; testable in isolation.
fn top_related_ids(mut with_sim: Vec<(String, f32)>, min_sim: f32, limit: usize) -> Vec<String> {
    with_sim.retain(|(_, s)| *s >= min_sim);
    with_sim.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    with_sim.into_iter().take(limit).map(|(id, _)| id).collect()
}

#[tauri::command(async)]
fn find_similar_entries(
    db: tauri::State<Db>,
    entry_id: String,
    limit: u32,
) -> Result<Vec<EntryPayload>, String> {
    ensure_embeddings_for_related_search(&db, &entry_id)?;
    let query_embedding = match db.get_embedding(&entry_id).map_err(|e| e.to_string())? {
        Some(v) => v,
        None => return Ok(vec![]),
    };
    let others = db
        .get_all_embeddings_excluding(&entry_id)
        .map_err(|e| e.to_string())?;
    let limit = if limit == 0 {
        usize::MAX
    } else {
        limit as usize
    };
    let with_sim: Vec<(String, f32)> = others
        .into_iter()
        .map(|(id, emb)| {
            let sim = embeddings::cosine_similarity(&query_embedding, &emb);
            (id, sim)
        })
        .collect();
    let top_ids = top_related_ids(with_sim, MIN_RELATED_SIMILARITY, limit);
    let rows = db.get_entries_by_ids(&top_ids).map_err(|e| e.to_string())?;
    let by_id: std::collections::HashMap<String, db::EntryRow> =
        rows.into_iter().map(|r| (r.id.clone(), r)).collect();
    let out: Vec<EntryPayload> = top_ids
        .into_iter()
        .filter_map(|id| by_id.get(&id))
        .map(|r| entry_row_to_payload(r))
        .collect();
    Ok(out)
}

#[tauri::command(async)]
fn list_entries(
    db: tauri::State<Db>,
    space_filter: Option<String>,
) -> Result<Vec<EntryPayload>, String> {
    let filter = resolve_space_filter(&db, space_filter)?;
    let rows = db
        .list_entries_filtered(&filter)
        .map_err(|e| e.to_string())?;
    Ok(rows.into_iter().map(|r| entry_row_to_payload(&r)).collect())
}

#[tauri::command(async)]
fn list_spaces(db: tauri::State<Db>) -> Result<Vec<SpacePayload>, String> {
    let rows = db.list_spaces().map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|r| SpacePayload {
            id: r.id,
            label: r.label,
            sort_order: r.sort_order,
        })
        .collect())
}

#[tauri::command(async)]
fn get_entry(db: tauri::State<Db>, entry_id: String) -> Result<Option<EntryPayload>, String> {
    let row = db
        .get_entry_by_id(&entry_id)
        .map_err(|e| e.to_string())?;
    Ok(row.as_ref().map(entry_row_to_payload))
}

#[tauri::command(async)]
fn jump_dates_in_month(
    db: tauri::State<Db>,
    year: i32,
    month: u32,
    space_filter: Option<String>,
) -> Result<Vec<String>, String> {
    if !(1970..=2100).contains(&year) || !(1..=12).contains(&month) {
        return Err("invalid year or month".to_string());
    }
    let filter = resolve_space_filter(&db, space_filter)?;
    db.local_entry_dates_in_month(year, month, &filter)
        .map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn jump_anchor_for_local_date(
    db: tauri::State<Db>,
    local_date: String,
    space_filter: Option<String>,
) -> Result<Option<String>, String> {
    let target = chrono::NaiveDate::parse_from_str(&local_date, "%Y-%m-%d")
        .map_err(|_| "invalid date (expected YYYY-MM-DD)".to_string())?;
    let filter = resolve_space_filter(&db, space_filter)?;
    db.jump_anchor_entry_id_for_local_date(target, &filter)
        .map_err(|e| e.to_string())
}

/// Temporal recall: try 24h, 7d, 30d anchors (±3h window); fallback to random past entry.
/// Delegates to recall::select_entry_for_resurface (pure, testable).
#[tauri::command(async)]
fn get_resurfaced_entry(
    db: tauri::State<Db>,
    exclude_ids: Vec<String>,
) -> Result<Option<ResurfacedPayload>, String> {
    get_resurfaced_entry_impl(&*db, exclude_ids, &mut rand::thread_rng())
}

/// Core resurface logic (DB + exclude list + RNG). Used by the command and by integration tests.
pub(crate) fn get_resurfaced_entry_impl<R: rand::RngCore>(
    db: &Db,
    exclude_ids: Vec<String>,
    rng: &mut R,
) -> Result<Option<ResurfacedPayload>, String> {
    let all_rows = db.list_entries().map_err(|e| e.to_string())?;
    let entries: Vec<recall::ResurfaceEntry> = all_rows
        .iter()
        .map(|r| recall::ResurfaceEntry {
            id: r.id.clone(),
            text: r.text.clone(),
            created_at: r.created_at.clone(),
            edit_count: r.edit_count,
            open_count: r.open_count,
            space_id: r.space_id.clone(),
        })
        .collect();
    let pinned_ids: std::collections::HashSet<String> = db
        .list_pinned_entry_ids()
        .map_err(|e| e.to_string())?
        .into_iter()
        .collect();
    let exclude: std::collections::HashSet<&str> = exclude_ids.iter().map(String::as_str).collect();
    let pinned_ref: std::collections::HashSet<&str> =
        pinned_ids.iter().map(String::as_str).collect();
    let now = chrono::Utc::now();
    let result = recall::select_entry_for_resurface(&entries, now, &exclude, &pinned_ref, rng);
    let (picked, anchor) = match result {
        Some(r) => r,
        None => return Ok(None),
    };
    let reason = match anchor {
        recall::Anchor::Anchor24h => temporal_reason_anchor("24h").to_string(),
        recall::Anchor::Anchor7d => temporal_reason_anchor("7d").to_string(),
        recall::Anchor::Anchor30d => temporal_reason_anchor("30d").to_string(),
        recall::Anchor::Fallback => {
            let created = parse_created_at(&picked.created_at).unwrap_or(now);
            format!("From {}.", format_ago(created, now))
        }
    };
    let row = db
        .get_entry_by_id(&picked.id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "entry not found".to_string())?;
    let linked = entries_with_trail_link_ids(&all_rows);
    let row = if linked.contains(&row.id) {
        row
    } else {
        let picked_ts = parse_created_at(&row.created_at).unwrap_or(now);
        let alts: Vec<&db::EntryRow> = all_rows
            .iter()
            .filter(|r| r.id != row.id && !exclude.contains(r.id.as_str()))
            .filter(|r| linked.contains(&r.id))
            .filter(|r| {
                let ts = parse_created_at(&r.created_at).unwrap_or(now);
                (picked_ts - ts).num_days().unsigned_abs() <= 30
            })
            .collect();
        if let Some(alt) = alts.choose(rng) {
            (*alt).clone()
        } else {
            row
        }
    };
    let trail_neighbor_count = thought_trail_neighbor_count_fast(&row, &all_rows);
    Ok(Some(ResurfacedPayload {
        entry: entry_row_to_payload(&row),
        reason,
        trail_neighbor_count: if trail_neighbor_count > 0 {
            Some(trail_neighbor_count)
        } else {
            None
        },
    }))
}

/// Thought trail: related entries ordered as earlier → current → later.
/// Scores by similarity (IDF-weighted keyword overlap) + temporal proximity; importance is a small boost.
#[tauri::command(async)]
fn get_thought_trail(db: tauri::State<Db>, entry_id: String) -> Result<Vec<EntryPayload>, String> {
    let current = db
        .get_entry_by_id(&entry_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "entry not found".to_string())?;
    let pinned_ids: std::collections::HashSet<String> = db
        .list_pinned_entry_ids()
        .map_err(|e| e.to_string())?
        .into_iter()
        .collect();
    let all = db.list_entries().map_err(|e| e.to_string())?;
    let rows = build_thought_trail_rows(&current, &all, &pinned_ids);
    Ok(rows
        .iter()
        .map(|r| {
            if r.id == current.id {
                entry_row_to_payload(r)
            } else {
                entry_row_to_trail_payload(r, &current.text)
            }
        })
        .collect())
}

/// Entry ids with enough keyword overlap for a stream trail dot (fast pairwise scan).
#[tauri::command(async)]
fn list_thought_trail_entry_ids(db: tauri::State<Db>) -> Result<Vec<String>, String> {
    let all = db.list_entries().map_err(|e| e.to_string())?;
    Ok(entries_with_trail_link_ids(&all).into_iter().collect())
}

fn entries_with_trail_link_ids(all: &[db::EntryRow]) -> std::collections::HashSet<String> {
    if all.len() < 2 {
        return std::collections::HashSet::new();
    }
    let min_overlap = thought_trail_min_overlap();
    let token_sets: Vec<std::collections::HashSet<String>> = all
        .iter()
        .map(|r| keywords::token_set(&r.text))
        .collect();
    let mut linked = std::collections::HashSet::new();
    for i in 0..all.len() {
        for j in (i + 1)..all.len() {
            if token_sets[i].intersection(&token_sets[j]).count() >= min_overlap {
                linked.insert(all[i].id.clone());
                linked.insert(all[j].id.clone());
            }
        }
    }
    linked
}

fn thought_trail_neighbor_count_fast(row: &db::EntryRow, all: &[db::EntryRow]) -> usize {
    let min_overlap = thought_trail_min_overlap();
    let max_related = thought_trail_max_related();
    select_thought_trail_candidate_rows(row, all)
        .into_iter()
        .filter(|r| keyword_overlap(&row.text, &r.text) >= min_overlap)
        .take(max_related)
        .count()
}

#[derive(serde::Serialize)]
struct CaptureContinuationHintPayload {
    entry_id: String,
    preview: String,
    days_earlier: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    shared_terms: Option<Vec<String>>,
}

/// Recent entry that strongly overlaps capture text (continuation nudge after save).
#[tauri::command(async)]
fn get_capture_continuation_hint(
    db: tauri::State<Db>,
    text: String,
    exclude_id: Option<String>,
) -> Result<Option<CaptureContinuationHintPayload>, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let min_overlap = capture_continuation_min_overlap();
    let max_days = capture_continuation_max_days();
    let now = chrono::Utc::now();
    let all = db.list_entries().map_err(|e| e.to_string())?;
    let mut best: Option<(db::EntryRow, usize, i64)> = None;
    for row in all {
        if exclude_id.as_deref() == Some(row.id.as_str()) {
            continue;
        }
        let overlap = keyword_overlap(trimmed, &row.text);
        if overlap < min_overlap {
            continue;
        }
        let ts = parse_created_at(&row.created_at).unwrap_or(now);
        let days = (now - ts).num_days();
        if days < 0 || days > max_days {
            continue;
        }
        let replace = match &best {
            None => true,
            Some((_prev, prev_overlap, prev_days)) => {
                overlap > *prev_overlap || (overlap == *prev_overlap && days < *prev_days)
            }
        };
        if replace {
            best = Some((row, overlap, days));
        }
    }
    Ok(best.map(|(row, _, days)| {
        let preview = if row.text.chars().count() > 80 {
            let end = row.text.char_indices().nth(80).map(|(i, _)| i).unwrap_or(row.text.len());
            format!("{}…", row.text[..end].trim_end())
        } else {
            row.text.clone()
        };
        let shared = shared_keywords(trimmed, &row.text, 5);
        CaptureContinuationHintPayload {
            entry_id: row.id,
            preview,
            days_earlier: days,
            shared_terms: if shared.is_empty() {
                None
            } else {
                Some(shared)
            },
        }
    }))
}

#[tauri::command(async)]
fn search_entries(
    db: tauri::State<Db>,
    query: String,
    space_filter: Option<String>,
    theme_filter: Option<String>,
) -> Result<Vec<SearchEntryPayload>, String> {
    let filter = resolve_space_filter(&db, space_filter)?;
    let rows = db
        .search_entries_filtered(&query, &filter, theme_filter.as_deref())
        .map_err(|e| e.to_string())?;
    let limit = keywords::default_topic_limit();
    Ok(rows
        .into_iter()
        .map(|r| {
            let topics = extract_keywords(&r.text, limit);
            SearchEntryPayload {
                id: r.id,
                text: r.text,
                created_at: r.created_at,
                highlighted: r.highlighted,
                topics: if topics.is_empty() {
                    None
                } else {
                    Some(topics)
                },
                space_id: r.space_id,
            }
        })
        .collect())
}

#[tauri::command(async)]
fn pin_entry(db: tauri::State<Db>, entry_id: String) -> Result<(), String> {
    db.get_entry_by_id(&entry_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "entry not found".to_string())?;
    db.insert_pinned(&entry_id).map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn unpin_entry(db: tauri::State<Db>, entry_id: String) -> Result<(), String> {
    db.remove_pinned(&entry_id).map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn get_pinned_entry_ids(db: tauri::State<Db>) -> Result<Vec<String>, String> {
    db.list_pinned_entry_ids().map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn mark_entry_continuation(
    db: tauri::State<Db>,
    entry_id: String,
    from_offset: i32,
    text: String,
) -> Result<Option<ContinuationMarkerPayload>, String> {
    db.get_entry_by_id(&entry_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "entry not found".to_string())?;
    let marked = db
        .mark_entry_continuation(&entry_id, from_offset, &text)
        .map_err(|e| e.to_string())?;
    Ok(marked.map(|(continuation_from, continuation_at)| ContinuationMarkerPayload {
        continuation_from,
        continuation_at,
    }))
}

#[derive(serde::Serialize)]
struct ContinuationMarkerPayload {
    continuation_from: i32,
    continuation_at: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateShareThreadInput {
    entry_ids: Vec<String>,
    context_note: Option<String>,
    #[serde(default = "default_share_expires_days")]
    expires_in_days: u32,
}

fn default_share_expires_days() -> u32 {
    14
}

#[derive(serde::Serialize)]
struct ShareThreadPayload {
    token: String,
    entry_ids: Vec<String>,
    context_note: Option<String>,
    created_at: String,
    expires_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    revoked_at: Option<String>,
}

fn share_thread_to_payload(row: db::ShareThreadRow) -> ShareThreadPayload {
    ShareThreadPayload {
        token: row.token,
        entry_ids: row.entry_ids,
        context_note: row.context_note,
        created_at: row.created_at,
        expires_at: row.expires_at,
        revoked_at: row.revoked_at,
    }
}

#[tauri::command(async)]
fn create_share_thread(
    db: tauri::State<Db>,
    input: CreateShareThreadInput,
) -> Result<ShareThreadPayload, String> {
    if input.entry_ids.is_empty() {
        return Err("select at least one thought".to_string());
    }
    if input.entry_ids.len() > db::MAX_SHARE_ENTRY_COUNT {
        return Err(format!(
            "at most {} thoughts per thread",
            db::MAX_SHARE_ENTRY_COUNT
        ));
    }
    let days = input.expires_in_days.clamp(1, 90);
    let rows = db
        .get_entries_by_ids(&input.entry_ids)
        .map_err(|e| e.to_string())?;
    if rows.len() != input.entry_ids.len() {
        return Err("one or more thoughts were not found".to_string());
    }
    let token = uuid::Uuid::new_v4().to_string();
    let created_at = chrono::Utc::now().to_rfc3339();
    let expires_at = (chrono::Utc::now() + chrono::Duration::days(days as i64)).to_rfc3339();
    let note = input
        .context_note
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    db.insert_share_thread(&token, &input.entry_ids, note, &created_at, &expires_at)
        .map_err(|e| e.to_string())?;
    Ok(ShareThreadPayload {
        token,
        entry_ids: input.entry_ids,
        context_note: note.map(str::to_string),
        created_at,
        expires_at,
        revoked_at: None,
    })
}

#[tauri::command(async)]
fn get_share_thread(
    db: tauri::State<Db>,
    token: String,
) -> Result<Option<ShareThreadPayload>, String> {
    let row = db
        .get_share_thread_row(&token)
        .map_err(|e| e.to_string())?;
    Ok(row
        .filter(db::share_thread_is_active)
        .map(share_thread_to_payload))
}

#[tauri::command(async)]
fn list_share_threads(db: tauri::State<Db>) -> Result<Vec<ShareThreadPayload>, String> {
    let rows = db.list_share_thread_rows().map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .filter(db::share_thread_is_active)
        .map(share_thread_to_payload)
        .collect())
}

#[tauri::command(async)]
fn revoke_share_thread(db: tauri::State<Db>, token: String) -> Result<(), String> {
    if db.revoke_share_thread(&token).map_err(|e| e.to_string())? {
        Ok(())
    } else {
        Err("thread not found or already revoked".to_string())
    }
}

#[tauri::command(async)]
fn write_utf8_file(path: String, contents: String) -> Result<(), String> {
    std::fs::write(path, contents).map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn update_entry(db: tauri::State<Db>, entry_id: String, text: String) -> Result<(), String> {
    db.get_entry_by_id(&entry_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "entry not found".to_string())?;
    db.update_entry_text(&entry_id, &text)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command(async)]
fn set_entry_space(
    db: tauri::State<Db>,
    entry_id: String,
    space_id: Option<String>,
) -> Result<(), String> {
    db.get_entry_by_id(&entry_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "entry not found".to_string())?;
    let sid = validated_space_for_write(&db, &space_id)?;
    db.update_entry_space(&entry_id, sid.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn record_entry_open(db: tauri::State<Db>, entry_id: String) -> Result<(), String> {
    db.record_entry_open(&entry_id).map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn delete_entry(db: tauri::State<Db>, entry_id: String) -> Result<(), String> {
    db.get_entry_by_id(&entry_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "entry not found".to_string())?;
    db.delete_entry(&entry_id).map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn delete_all_entries(db: tauri::State<Db>) -> Result<(), String> {
    db.delete_all_entries().map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn export_entries(db: tauri::State<Db>, path: String) -> Result<(), String> {
    let mut rows = db.list_entries().map_err(|e| e.to_string())?;
    rows.reverse();
    let dest = PathBuf::from(&path);
    let file = File::create(&dest).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let opts =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for row in rows {
        let dt = parse_created_at(&row.created_at)
            .ok_or_else(|| format!("invalid created_at: {}", row.created_at))?;
        let filename = format!("{}.md", dt.format("%Y-%m-%d-%H-%M-%S"));
        let space_line = match &row.space_id {
            Some(s) => format!("space_id: {}\n", s),
            None => String::new(),
        };
        let frontmatter = format!(
            "---\ncreated_at: {}\n{}app: chinotto\nversion: {}\n---\n\n",
            row.created_at,
            space_line,
            env!("CARGO_PKG_VERSION")
        );
        let content = format!("{}{}", frontmatter, row.text);
        let entry_path = format!("chinotto-export/entries/{}", filename);
        zip.start_file(entry_path, opts)
            .map_err(|e| e.to_string())?;
        zip.write_all(content.as_bytes())
            .map_err(|e| e.to_string())?;
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

/// The whole Record, as plain text and audio, in one zip in Downloads.
///
/// Plain text on purpose. An export exists so the record can outlive this program, and a
/// format only this program reads is not an escape hatch. The audio travels beside it
/// because the recording is the material — the transcript is derived from it, and an export
/// that kept only the derivation would be throwing the original away.
#[tauri::command(async)]
fn export_record(db: tauri::State<Db>, app: tauri::AppHandle) -> Result<String, String> {
    let mut fragments = db.recent_fragments(1_000_000).map_err(|e| e.to_string())?;
    // Oldest first: an export is read forwards.
    fragments.reverse();
    let ids: Vec<String> = fragments.iter().map(|f| f.id.clone()).collect();
    let (_encounters, voices) = db.materials_for(&ids).map_err(|e| e.to_string())?;

    let downloads = app
        .path()
        .download_dir()
        .map_err(|e| e.to_string())?;
    fs::create_dir_all(&downloads).map_err(|e| e.to_string())?;
    let name = "chinotto-record.zip".to_string();
    let dest = downloads.join(&name);

    let file = File::create(&dest).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let opts =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let mut text = String::new();
    for f in &fragments {
        text.push_str(&f.captured_at);
        text.push_str(" · ");
        text.push_str(&f.capture_method);
        if let Some(origin) = &f.capture_origin {
            text.push_str(" · ");
            text.push_str(origin);
        }
        if f.correction_count > 0 {
            text.push_str(" · wording corrected");
        }
        text.push('\n');
        text.push_str(&f.body);
        text.push_str("\n\n");
    }
    zip.start_file("chinotto-record/record.txt", opts)
        .map_err(|e| e.to_string())?;
    zip.write_all(text.as_bytes()).map_err(|e| e.to_string())?;

    for v in &voices {
        // A recording whose file is gone is simply not in the export; the words for it are
        // already in record.txt, and writing an empty file would claim otherwise.
        let src = PathBuf::from(&v.audio_path);
        let Ok(bytes) = fs::read(&src) else { continue };
        let ext = src
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("m4a");
        zip.start_file(format!("chinotto-record/audio/{}.{}", v.fragment_id, ext), opts)
            .map_err(|e| e.to_string())?;
        zip.write_all(&bytes).map_err(|e| e.to_string())?;
    }

    zip.finish().map_err(|e| e.to_string())?;
    Ok(name)
}

/// When the most recent automatic backup was taken, so settings can say so truthfully.
#[tauri::command(async)]
fn last_backup_at(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let (_db_path, backups_dir) = backup_paths(&app)?;
    let Ok(entries) = fs::read_dir(&backups_dir) else {
        return Ok(None);
    };
    let mut newest: Option<std::time::SystemTime> = None;
    for e in entries.flatten() {
        let Ok(meta) = e.metadata() else { continue };
        let Ok(modified) = meta.modified() else { continue };
        if newest.map_or(true, |n| modified > n) {
            newest = Some(modified);
        }
    }
    Ok(newest.map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339()))
}

/// Opens System Settings at the microphone pane.
///
/// The product cannot grant itself the microphone and must not pretend otherwise: when the
/// mac has said no, the only honest affordance is the door to where the answer lives.
#[tauri::command(async)]
fn open_microphone_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Shows the menu-bar capture panel, for settings' "try it".
#[tauri::command(async)]
fn open_tray_capture(app: tauri::AppHandle) -> Result<(), String> {
    let _ = app.emit("chinotto-capture-shortcut", ());
    Ok(())
}

const BACKUP_RETENTION_COUNT: usize = 7;
const AUTO_BACKUP_COOLDOWN_HOURS: i64 = 24;

fn backup_paths(app: &tauri::AppHandle) -> Result<(PathBuf, PathBuf), String> {
    let base = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = base.join("chinotto.db");
    let backups_dir = base.join("chinotto-backups");
    Ok((db_path, backups_dir))
}

fn prune_old_backups(backups_dir: &std::path::Path) -> Result<(), String> {
    let mut entries: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    for e in fs::read_dir(backups_dir).map_err(|e| e.to_string())? {
        let e = e.map_err(|e| e.to_string())?;
        let path = e.path();
        if path.extension().map_or(false, |e| e == "db") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if let Some(suffix) = stem.strip_prefix("chinotto-") {
                    if chrono::NaiveDateTime::parse_from_str(suffix, "%Y-%m-%d-%H-%M").is_ok() {
                        if let Ok(meta) = e.metadata() {
                            if let Ok(modified) = meta.modified() {
                                entries.push((modified, path));
                            }
                        }
                    }
                }
            }
        }
    }
    if entries.len() <= BACKUP_RETENTION_COUNT {
        return Ok(());
    }
    entries.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, path) in entries.into_iter().skip(BACKUP_RETENTION_COUNT) {
        let _ = fs::remove_file(&path);
    }
    Ok(())
}

#[tauri::command(async)]
fn create_backup(app: tauri::AppHandle) -> Result<(), String> {
    let (db_path, backups_dir) = backup_paths(&app)?;
    if !db_path.exists() {
        return Err("Database file not found.".to_string());
    }
    fs::create_dir_all(&backups_dir).map_err(|e| e.to_string())?;
    let timestamp = chrono::Utc::now().format("%Y-%m-%d-%H-%M");
    let backup_name = format!("chinotto-{}.db", timestamp);
    let backup_path = backups_dir.join(&backup_name);
    fs::copy(&db_path, &backup_path).map_err(|e| e.to_string())?;
    prune_old_backups(&backups_dir)?;
    Ok(())
}

#[tauri::command(async)]
fn create_backup_if_needed(app: tauri::AppHandle) -> Result<(), String> {
    let (db_path, backups_dir) = backup_paths(&app)?;
    if !db_path.exists() {
        return Ok(());
    }
    if !backups_dir.exists() {
        return create_backup(app);
    }
    let mut latest: Option<chrono::DateTime<chrono::Utc>> = None;
    for e in fs::read_dir(&backups_dir).map_err(|e| e.to_string())? {
        let e = e.map_err(|e| e.to_string())?;
        let path = e.path();
        if path.extension().map_or(false, |e| e == "db") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if let Some(suffix) = stem.strip_prefix("chinotto-") {
                    if let Ok(naive) =
                        chrono::NaiveDateTime::parse_from_str(suffix, "%Y-%m-%d-%H-%M")
                    {
                        let dt = chrono::Utc.from_utc_datetime(&naive);
                        if latest.map_or(true, |l| dt > l) {
                            latest = Some(dt);
                        }
                    }
                }
            }
        }
    }
    let need = match latest {
        None => true,
        Some(l) => {
            chrono::Utc::now().signed_duration_since(l).num_hours() >= AUTO_BACKUP_COOLDOWN_HOURS
        }
    };
    if need {
        create_backup(app)
    } else {
        Ok(())
    }
}

#[derive(serde::Serialize)]
struct EntryPayload {
    id: String,
    text: String,
    created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    continuation_from: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    continuation_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    topics: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    space_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trail_shared: Option<Vec<String>>,
}

fn entry_row_to_payload(r: &db::EntryRow) -> EntryPayload {
    let limit = keywords::default_topic_limit();
    let topics = extract_keywords(&r.text, limit);
    EntryPayload {
        id: r.id.clone(),
        text: r.text.clone(),
        created_at: r.created_at.clone(),
        updated_at: if r.updated_at.is_empty() {
            None
        } else {
            Some(r.updated_at.clone())
        },
        continuation_from: r.continuation_from,
        continuation_at: r.continuation_at.clone(),
        topics: if topics.is_empty() {
            None
        } else {
            Some(topics)
        },
        space_id: r.space_id.clone(),
        trail_shared: None,
    }
}

fn entry_row_to_trail_payload(r: &db::EntryRow, current_text: &str) -> EntryPayload {
    let mut payload = entry_row_to_payload(r);
    let shared = shared_keywords(current_text, &r.text, 5);
    if !shared.is_empty() {
        payload.trail_shared = Some(shared);
    }
    payload
}

fn thought_trail_score_row(
    current: &db::EntryRow,
    current_ts: chrono::DateTime<chrono::Utc>,
    r: &db::EntryRow,
    corpus: &[std::collections::HashSet<String>],
    pinned_ids: &std::collections::HashSet<String>,
) -> f64 {
    let sim = thought_trail_similarity(&current.text, &r.text, corpus, 15);
    let other_ts = parse_created_at(&r.created_at).unwrap_or(current_ts);
    let days = (current_ts - other_ts).num_days().unsigned_abs() as f64;
    let time_score = 1.0 / (1.0 + days);
    let mut base = 0.6 * sim + 0.4 * time_score;
    let continuation_active =
        current.continuation_from.is_some() || r.continuation_from.is_some();
    if continuation_active && days <= 7.0 {
        base *= 1.08;
    }
    base * importance_boost(importance_score(r, pinned_ids))
}

fn select_thought_trail_candidate_rows<'a>(
    current: &'a db::EntryRow,
    all: &'a [db::EntryRow],
) -> Vec<db::EntryRow> {
    let mut others: Vec<&db::EntryRow> = all.iter().filter(|r| r.id != current.id).collect();
    others.sort_by(|a, b| {
        keyword_overlap(&current.text, &b.text)
            .cmp(&keyword_overlap(&current.text, &a.text))
            .then_with(|| b.created_at.cmp(&a.created_at))
    });
    others
        .into_iter()
        .take(thought_trail_candidates())
        .cloned()
        .collect()
}

fn build_thought_trail_rows(
    current: &db::EntryRow,
    all: &[db::EntryRow],
    pinned_ids: &std::collections::HashSet<String>,
) -> Vec<db::EntryRow> {
    let current_ts = parse_created_at(&current.created_at).unwrap_or_else(chrono::Utc::now);
    let candidates = select_thought_trail_candidate_rows(current, all);
    let corpus: Vec<std::collections::HashSet<String>> = candidates
        .iter()
        .map(|r| keywords::token_set(&r.text))
        .collect();
    let min_overlap = thought_trail_min_overlap();
    let max_related = thought_trail_max_related();

    let mut scored: Vec<(db::EntryRow, f64)> = candidates
        .into_iter()
        .filter(|r| keyword_overlap(&current.text, &r.text) >= min_overlap)
        .map(|r| {
            let score = thought_trail_score_row(current, current_ts, &r, &corpus, pinned_ids);
            (r, score)
        })
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let top: Vec<db::EntryRow> = scored
        .into_iter()
        .take(max_related)
        .map(|(row, _)| row)
        .collect();

    let (mut before, mut after): (Vec<db::EntryRow>, Vec<db::EntryRow>) = top.into_iter().partition(|r| {
        parse_created_at(&r.created_at)
            .map(|t| t < current_ts)
            .unwrap_or(false)
    });
    before.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    after.sort_by(|a, b| a.created_at.cmp(&b.created_at));

    let mut out = before;
    out.push(current.clone());
    out.extend(after);
    out
}

#[derive(serde::Serialize)]
struct SearchEntryPayload {
    id: String,
    text: String,
    created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    highlighted: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    topics: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    space_id: Option<String>,
}

#[derive(serde::Serialize)]
struct SpacePayload {
    id: String,
    label: String,
    sort_order: i32,
}

#[derive(serde::Serialize)]
struct ResurfacedPayload {
    entry: EntryPayload,
    reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    trail_neighbor_count: Option<usize>,
}

#[cfg(target_os = "macos")]
struct SpeechCommandTx(Arc<mpsc::SyncSender<speech::SpeechCommand>>);

/// What a finished capture left behind, for the surface to turn into a fragment.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceCaptureResult {
    pub audio_path: String,
    pub duration_ms: u64,
    /// `null` when nothing was transcribed. The recording is still there.
    pub transcript: Option<String>,
}

/// Where recordings live. Beside the record, because they are part of it.
fn audio_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("audio");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// Hold to speak. Records to a file and transcribes it if it can.
///
/// The recording is the material and it is written as it arrives, so a recogniser that is
/// unauthorised, fails or times out costs the words but never the audio. Only a failure to
/// record at all is an error here.
#[tauri::command(async)]
fn run_native_speech_recognition(
    app: tauri::AppHandle,
    max_ms: Option<u64>,
) -> Result<VoiceCaptureResult, String> {
    #[cfg(target_os = "macos")]
    {
        let max_ms = max_ms.unwrap_or(120_000);
        let path = audio_dir(&app)?.join(format!("{}.caf", uuid::Uuid::new_v4()));

        let (result_tx, result_rx) = mpsc::sync_channel(1);
        let (event_tx, event_rx) = mpsc::sync_channel(4);
        let app_handle = app.clone();
        std::thread::spawn(move || {
            while let Ok(state) = event_rx.recv() {
                let _ = app_handle.emit("chinotto-speech-state", state);
            }
        });
        let cmd_tx = app.state::<SpeechCommandTx>().0.clone();
        cmd_tx
            .send((max_ms, path, result_tx, Some(event_tx)))
            .map_err(|_| "the voice pipeline is not running".to_string())?;
        match result_rx.recv_timeout(std::time::Duration::from_secs(180)) {
            Ok(Ok(capture)) => Ok(VoiceCaptureResult {
                audio_path: capture.audio_path.to_string_lossy().into_owned(),
                duration_ms: capture.duration_ms,
                transcript: capture.transcript,
            }),
            Ok(Err(e)) => Err(e),
            Err(_) => Err("the recording did not come back".to_string()),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, max_ms);
        Err("Voice capture is only available on macOS".to_string())
    }
}

/// The hold was released. Ends the recording that is running, if any.
#[tauri::command(async)]
fn stop_voice_capture() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    speech::request_stop();
    Ok(())
}

#[tauri::command]
fn set_app_icon(app: tauri::AppHandle, png_base64: String) -> Result<(), String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(png_base64.trim())
        .map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    set_macos_dock_icon(&bytes)?;
    #[cfg(not(target_os = "macos"))]
    {
        let window = app
            .get_webview_window("main")
            .ok_or("main window not found")?;
        window
            .set_icon(tauri::Icon::Raw(bytes))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn set_macos_dock_icon(png_bytes: &[u8]) -> Result<(), String> {
    use objc2::rc::Allocated;
    use objc2::{msg_send, ClassType, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::{NSData, NSSize};

    unsafe {
        let mtm = MainThreadMarker::new().ok_or("Not on main thread")?;
        let app = NSApplication::sharedApplication(mtm);
        let data = NSData::with_bytes(png_bytes);
        let alloc: Allocated<NSImage> = msg_send![NSImage::class(), alloc];
        let image = NSImage::initWithData(alloc, &data).ok_or("NSImage initWithData failed")?;
        // PNG is 1024×1024 px. Without setSize, AppKit maps ~1 px = 1 pt (Dock tile too large).
        // 512×512 pt is the logical size for a 1024 px @2x app icon (matches bundle safe-area artwork).
        const DOCK_ICON_LOGICAL_PTS: f64 = 512.0;
        image.setSize(NSSize::new(DOCK_ICON_LOGICAL_PTS, DOCK_ICON_LOGICAL_PTS));
        app.setApplicationIconImage(Some(&image));
    }
    Ok(())
}

/// Speak from anywhere on the mac.
///
/// Hold is the model, so there is one voice chord and it is a hold. `⌘⇧V` is gone: a
/// press-to-start/press-to-stop shortcut taught a different gesture from the one at the
/// edge, and two gestures for one action is one too many.
const VOICE_HOLD: &str = "Alt+Space";
const CAPTURE_SHORTCUT: &str = "CommandOrControl+Shift+K";

/// Show `main` or recreate it from config if the webview was torn down.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn ensure_main_window_focus(app: &tauri::AppHandle) {
    use tauri::WebviewWindowBuilder;

    if let Some(main) = app.get_webview_window("main") {
        let _ = main.unminimize();
        let _ = main.show();
        let _ = main.set_focus();
        return;
    }
    let Some(cfg) = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == "main")
    else {
        log::warn!("ensure_main_window_focus: missing main window config");
        return;
    };
    match WebviewWindowBuilder::from_config(app, cfg) {
        Ok(builder) => match builder.build() {
            Ok(w) => {
                let _ = w.show();
                let _ = w.set_focus();
            }
            Err(e) => log::warn!("ensure_main_window_focus: failed to build main window: {e}"),
        },
        Err(e) => log::warn!("ensure_main_window_focus: invalid main window builder: {e}"),
    }
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn ensure_main_window_focus(_app: &tauri::AppHandle) {}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    use std::str::FromStr;
    use tauri::Emitter;
    use tauri::Manager;
    use tauri_plugin_global_shortcut::{Shortcut, ShortcutState};

    let voice_hold_id = Shortcut::from_str(VOICE_HOLD).ok().map(|s| s.id());
    let capture_shortcut_id = Shortcut::from_str(CAPTURE_SHORTCUT).ok().map(|s| s.id());

    let voice_handler =
        move |app: &tauri::AppHandle,
              shortcut: &tauri_plugin_global_shortcut::Shortcut,
              event: tauri_plugin_global_shortcut::ShortcutEvent| {
            let id = shortcut.id();
            // Hold is the model: press starts the recording, release ends it. There is no
            // press-to-start/press-to-stop variant, here or at the edge.
            let _ = match (voice_hold_id, &event.state) {
                (Some(hid), ShortcutState::Pressed) if id == hid => {
                    ensure_main_window_focus(app);
                    app.emit("chinotto-voice-hold-start", ())
                }
                (Some(hid), ShortcutState::Released) if id == hid => {
                    app.emit("chinotto-voice-hold-stop", ())
                }
                _ => Ok(()),
            };

            if matches!(event.state, ShortcutState::Pressed) && Some(id) == capture_shortcut_id {
                ensure_main_window_focus(app);
                let _ = app.emit("chinotto-capture-shortcut", ());
            }
        };

    let shortcuts: Vec<&str> = vec![CAPTURE_SHORTCUT, VOICE_HOLD];
    let plugin_builder = tauri_plugin_global_shortcut::Builder::new()
        .with_shortcuts(shortcuts)
        .expect("shortcuts")
        .with_handler(voice_handler)
        .build();

    tauri::Builder::default()
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(plugin_builder)
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            let path = app
                .handle()
                .path()
                .app_data_dir()
                .map_err(|e| e.to_string())?;
            fs::create_dir_all(&path).map_err(|e| e.to_string())?;
            let db_path = path.join("chinotto.db");
            let db = Db::open(db_path).map_err(|e| e.to_string())?;
            app.manage(db);
            // Before anything can ask for a guess. fastembed would otherwise cache the
            // weights beside the working directory, which for an app opened from Finder is
            // `/` — so they would be fetched, fail to store, and be fetched again forever.
            embeddings::set_cache_dir(path.join("models"));
            #[cfg(target_os = "macos")]
            {
                let (cmd_tx, cmd_rx) = mpsc::sync_channel(0);
                app.manage(SpeechCommandTx(Arc::new(cmd_tx)));
                std::thread::spawn(move || speech::run_speech_loop(cmd_rx));
            }
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            tray_capture::setup(app)?;
            // After in-app update (`install` + `relaunch`), macOS can restore the main window
            // minimized or behind other apps; force a visible, focused main window on startup.
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            if let Some(main) = app.handle().get_webview_window("main") {
                let _ = main.unminimize();
                let _ = main.show();
                let _ = main.set_focus();
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // The Record
            record_commands::capture_fragment,
            record_commands::continue_fragment,
            record_commands::link_continuation,
            record_commands::correct_fragment,
            record_commands::fragment_history,
            record_commands::line_for,
            record_commands::hold_fragment,
            record_commands::max_held,
            record_commands::release_fragment,
            record_commands::held_fragments,
            record_commands::recent_fragments,
            record_commands::fragments_before,
            record_commands::fragments_between,
            record_commands::find_fragments,
            record_commands::month_density,
            record_commands::record_span,
            record_commands::select_return,
            record_commands::record_return_outcome,
            record_commands::find_by_meaning,
            record_commands::reject_guess,
            record_commands::embed_pending,
            record_commands::pending_embedding_count,
            record_commands::capture_encounter,
            record_commands::encounter_for,
            record_commands::same_source_encounters,
            record_commands::encounters_awaiting_enrichment,
            record_commands::record_enrichment,
            record_commands::capture_voice,
            record_commands::record_transcript,
            record_commands::voice_for,
            record_commands::mark_audio_missing,
            record_commands::materials_for,
            record_commands::mirror_pending_fragments,
            record_commands::fragments_awaiting_mirror,
            record_commands::absorb_remote_deletes,
            record_commands::project_entries_into_record,
            record_commands::this_device,
            record_commands::open_wording_conflicts,
            record_commands::resolve_wording_conflict,
            record_commands::restore_fragment,
            record_commands::remove_fragment,
            native_apple_sign_in,
            oauth_dev_bridge::start_oauth_dev_bridge_listener,
            ingest_firestore_entries,
            enqueue_sync_tombstone,
            list_sync_tombstone_outbox,
            list_due_sync_tombstone_outbox,
            remove_sync_tombstone_outbox,
            clear_sync_tombstone_outbox_all,
            clear_firestore_ingest_suppression,
            delete_local_entries_for_sync,
            apply_remote_entry_theme,
            ingest_remote_user_themes,
            apply_remote_user_theme_tombstones,
            list_sync_user_theme_outbox,
            remove_sync_user_theme_outbox,
            clear_sync_user_theme_outbox_all,
            clear_user_theme_ingest_suppression,
            enqueue_all_local_user_themes_for_sync,
            list_entry_ids_with_themes,
            create_entry,
            restore_entry,
            update_entry,
            mark_entry_continuation,
            set_entry_space,
            list_entries,
            list_spaces,
            get_entry,
            jump_dates_in_month,
            jump_anchor_for_local_date,
            search_entries,
            run_native_speech_recognition,
            stop_voice_capture,
            generate_embedding,
            classify_entry_theme,
            get_entry_theme,
            set_entry_theme,
            list_user_themes,
            create_user_theme,
            update_user_theme,
            delete_user_theme,
            list_theme_counts,
            list_theme_counts_recent,
            find_similar_entries,
            get_resurfaced_entry,
            get_thought_trail,
            list_thought_trail_entry_ids,
            get_capture_continuation_hint,
            pin_entry,
            unpin_entry,
            get_pinned_entry_ids,
            record_entry_open,
            delete_entry,
            delete_all_entries,
            export_entries,
            export_record,
            last_backup_at,
            open_microphone_settings,
            open_tray_capture,
            create_share_thread,
            get_share_thread,
            list_share_threads,
            revoke_share_thread,
            write_utf8_file,
            create_backup,
            create_backup_if_needed,
            set_app_icon,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            match &event {
                tauri::RunEvent::WindowEvent { label, event: win_evt, .. }
                    if label == "main" =>
                {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = win_evt {
                        api.prevent_close();
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.hide();
                        }
                    }
                }
                _ => {}
            }
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = &event {
                ensure_main_window_focus(app);
            }
            #[cfg(any(target_os = "android", target_os = "ios"))]
            drop(event);
        });
}

#[cfg(test)]
mod related_entries_tests {
    use super::*;

    #[test]
    fn threshold_filters_low_similarity_before_sort_and_limit() {
        let with_sim = vec![
            ("weak".to_string(), 0.35),
            ("strong".to_string(), 0.72),
            ("boundary".to_string(), 0.5),
            ("noise".to_string(), 0.41),
            ("good".to_string(), 0.58),
        ];
        let ids = top_related_ids(with_sim, MIN_RELATED_SIMILARITY, 3);
        assert_eq!(ids, ["strong", "good", "boundary"]);
    }

    #[test]
    fn no_results_when_all_below_threshold() {
        let with_sim = vec![("a".to_string(), 0.3), ("b".to_string(), 0.4)];
        let ids = top_related_ids(with_sim, MIN_RELATED_SIMILARITY, 5);
        assert!(ids.is_empty());
    }

    #[test]
    fn limit_respected_after_filtering() {
        let with_sim = vec![
            ("1".to_string(), 0.9),
            ("2".to_string(), 0.8),
            ("3".to_string(), 0.7),
            ("4".to_string(), 0.6),
            ("5".to_string(), 0.55),
        ];
        let ids = top_related_ids(with_sim, MIN_RELATED_SIMILARITY, 2);
        assert_eq!(ids.len(), 2);
        assert_eq!(ids, ["1", "2"]);
    }

    #[test]
    fn find_similar_indexes_missing_embeddings_then_matches_pair() {
        let db = Db::open(std::path::PathBuf::from(":memory:")).unwrap();
        let t1 = "2026-05-25T22:34:00Z";
        let t2 = "2026-05-25T22:37:00Z";
        let a = "Working on the Chinotto macOS app today. Tauri 2 shell, React UI, SQLite with FTS5 for search.";
        let b = "Chinotto desktop: Tauri backend, React frontend, local SQLite. Testing whether embedding similarity shows related entries in the detail view.";
        db.create_entry("a", a, t1, None).unwrap();
        db.create_entry("b", b, t2, None).unwrap();
        ensure_embeddings_for_related_search(&db, "b").unwrap();
        let query = db.get_embedding("b").unwrap();
        assert!(query.is_some(), "current entry should be embedded");
        let others = db.get_all_embeddings_excluding("b").unwrap();
        assert!(!others.is_empty(), "peer entry should be embedded");
        let with_sim: Vec<(String, f32)> = others
            .into_iter()
            .map(|(id, emb)| {
                let sim = embeddings::cosine_similarity(query.as_ref().unwrap(), &emb);
                (id, sim)
            })
            .collect();
        let ids = top_related_ids(with_sim, MIN_RELATED_SIMILARITY, 5);
        assert_eq!(ids, vec!["a"]);
    }
}

#[cfg(test)]
mod resurface_integration {
    use super::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;
    use std::path::PathBuf;

    fn open_memory_db() -> Db {
        Db::open(PathBuf::from(":memory:")).expect("in-memory db")
    }

    #[test]
    fn at_most_one_entry_returned_per_call() {
        let db = open_memory_db();
        let now = chrono::Utc::now();
        let t24 = (now - chrono::Duration::days(1)).to_rfc3339();
        db.create_entry("a", "entry a", &t24, None).unwrap();
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let r = get_resurfaced_entry_impl(&db, vec![], &mut rng).unwrap();
        assert!(r.is_some());
        assert_eq!(r.as_ref().map(|p| p.entry.id.as_str()), Some("a"));
    }

    #[test]
    fn excluded_ids_never_returned() {
        let db = open_memory_db();
        let now = chrono::Utc::now();
        let t24 = (now - chrono::Duration::days(1)).to_rfc3339();
        let t7d = (now - chrono::Duration::days(7)).to_rfc3339();
        db.create_entry("id-24h", "thought 24h", &t24, None).unwrap();
        db.create_entry("id-7d", "thought 7d", &t7d, None).unwrap();
        let mut rng = ChaCha8Rng::seed_from_u64(2);
        let with_exclude =
            get_resurfaced_entry_impl(&db, vec!["id-24h".to_string()], &mut rng).unwrap();
        assert!(with_exclude.is_some());
        assert_eq!(with_exclude.as_ref().unwrap().entry.id, "id-7d");
    }

    #[test]
    fn cooldown_excluding_only_candidate_returns_none() {
        let db = open_memory_db();
        let now = chrono::Utc::now();
        let t24 = (now - chrono::Duration::days(1)).to_rfc3339();
        db.create_entry("only", "only entry", &t24, None).unwrap();
        let mut rng = ChaCha8Rng::seed_from_u64(3);
        let r = get_resurfaced_entry_impl(&db, vec!["only".to_string()], &mut rng).unwrap();
        assert!(r.is_none());
    }

    #[test]
    fn empty_db_returns_none() {
        let db = open_memory_db();
        let mut rng = ChaCha8Rng::seed_from_u64(4);
        let r = get_resurfaced_entry_impl(&db, vec![], &mut rng).unwrap();
        assert!(r.is_none());
    }
}
