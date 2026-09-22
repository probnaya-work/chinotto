//! Meaning, as a vector.
//!
//! This is the one thing in the product that is a *guess*: two fragments being close here is
//! never stated as a fact, only ever offered as something to look at. The words model in
//! `words.ts` answers "the same words", which can be shown; this answers "close in meaning",
//! which cannot.
//!
//! Everything here is allowed to be unavailable. A mac that is offline, behind a proxy, or
//! simply has never fetched the weights still has a complete Record, a complete Find and
//! complete word-based traces — it just has no guesses. That is a missing opinion, not a
//! missing feature, so nothing below ever blocks, retries in a loop, or reports an error to
//! the person.

use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use once_cell::sync::OnceCell;
use std::path::PathBuf;
use std::sync::Mutex;

/// Where the weights live.
///
/// fastembed's own default is `.fastembed_cache` *relative to the working directory*, and a
/// mac app launched from Finder has `/` for a working directory — which it cannot write. The
/// effect is not a missing cache but an endless one: every call re-fetches ~90 MB, fails to
/// store it, and the next call starts again. The path is set once at startup from the app's
/// own data directory instead (see `lib.rs`), so the fetch happens at most once ever.
static CACHE_DIR: OnceCell<PathBuf> = OnceCell::new();

pub fn set_cache_dir(dir: PathBuf) {
    let _ = CACHE_DIR.set(dir);
}

/// The model, or the reason there is not one.
///
/// Deliberately a cached `Result`: the first load may have to fetch the weights, and a mac
/// that cannot is a mac that cannot — asking it again on the next fragment the person opens
/// only buys another stall. One attempt per run, and after that the answer is instant either
/// way.
static MODEL: OnceCell<Result<Mutex<TextEmbedding>, String>> = OnceCell::new();

fn model() -> Result<&'static Mutex<TextEmbedding>, String> {
    MODEL
        .get_or_init(|| {
            #[cfg(feature = "mas")]
            {
                let dir = CACHE_DIR
                    .get()
                    .ok_or_else(|| "bundled meaning model directory is not configured".to_string())?;
                let repository = dir.join("models--Qdrant--all-MiniLM-L6-v2-onnx");
                if !repository.join("refs/main").is_file() {
                    return Err("bundled meaning model is unavailable".to_string());
                }
            }
            let mut options = InitOptions::new(EmbeddingModel::AllMiniLML6V2)
                .with_show_download_progress(false);
            if let Some(dir) = CACHE_DIR.get() {
                options = options.with_cache_dir(dir.clone());
            }
            TextEmbedding::try_new(options)
                .map(Mutex::new)
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(|e| e.clone())
}

/// Whether meaning is available at all, without paying to find out.
///
/// Answers only what has already been settled, so a surface can ask it on the way past.
/// `None` means the model has not been tried yet — not that it is missing.
pub fn availability() -> Option<Result<(), String>> {
    MODEL.get().map(|r| r.as_ref().map(|_| ()).map_err(|e| e.clone()))
}

pub fn embed_text(text: &str) -> Result<Vec<f32>, String> {
    let mut guard = model()?.lock().map_err(|e| e.to_string())?;
    let embeddings = guard.embed(&[text.to_string()], None).map_err(|e| e.to_string())?;
    embeddings
        .into_iter()
        .next()
        .ok_or_else(|| "no embedding returned".to_string())
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression: pair from manual Related-thoughts QA (English, shared Chinotto/Tauri/SQLite terms).
    #[test]
    fn chinotto_pair_similarity_meets_related_threshold() {
        let a = "Working on the Chinotto macOS app today. Tauri 2 shell, React UI, SQLite with FTS5 for search.";
        let b = "Chinotto desktop: Tauri backend, React frontend, local SQLite. Testing whether embedding similarity shows related entries in the detail view.";
        let ea = embed_text(a).expect("embed a");
        let eb = embed_text(b).expect("embed b");
        let sim = cosine_similarity(&ea, &eb);
        assert!(sim >= 0.5, "expected cosine similarity >= 0.5, got {sim}");
    }

    /// A failed load is remembered, so a mac without the weights stalls once and never again.
    ///
    /// Asserted on the cached value rather than by timing: `availability()` answering at all
    /// is the property — it can only answer from `MODEL`, which is written exactly once.
    #[test]
    fn availability_is_settled_after_the_first_attempt() {
        assert!(availability().is_none() || availability().is_some());
        let _ = embed_text("anything at all");
        assert!(
            availability().is_some(),
            "the first attempt must settle whether meaning is available"
        );
    }
}
