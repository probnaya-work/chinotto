//! User-selected archives are verified in private staging before any Record write.
use crate::{
    db::{
        archive::{self, Archive, Preview},
        Db,
    },
    privacy::Privacy,
};
use serde::Serialize;
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tauri::Manager;

struct Prepared {
    archive: Archive,
    preview: Preview,
    directory: PathBuf,
    epoch: u64,
}
impl Drop for Prepared {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}
#[derive(Default)]
struct Work {
    cancelled: AtomicBool,
    prepared: Mutex<Option<Prepared>>,
}
#[derive(Default)]
pub struct Imports(Mutex<HashMap<String, Arc<Work>>>);
#[derive(Serialize)]
pub struct PreparedPreview {
    token: String,
    preview: Preview,
}

fn check(app: &tauri::AppHandle, epoch: u64, work: &Work) -> Result<(), String> {
    let state = app.state::<Privacy>().snapshot()?;
    if work.cancelled.load(Ordering::Acquire) || state.locked || state.epoch != epoch {
        Err("Restore cancelled. Your existing Record is unchanged.".into())
    } else {
        Ok(())
    }
}
pub fn setup(app: &tauri::AppHandle) -> Result<(), String> {
    let root = app
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?
        .join("record-imports");
    if root.exists() {
        fs::remove_dir_all(root).map_err(|e| e.to_string())?;
    }
    app.manage(Imports::default());
    Ok(())
}
#[tauri::command(async)]
pub fn prepare_record_restore(
    app: tauri::AppHandle,
    path: String,
    token: String,
) -> Result<PreparedPreview, String> {
    let token = uuid::Uuid::parse_str(&token)
        .map_err(|_| "Invalid import request")?
        .to_string();
    let epoch = app.state::<Privacy>().snapshot()?.epoch;
    let work = Arc::new(Work::default());
    {
        let imports = app.state::<Imports>();
        let mut pending = imports.0.lock().map_err(|_| "Import unavailable")?;
        // One review at a time. Superseded reads cannot leave a private preview behind.
        for (_, old) in pending.drain() {
            old.cancelled.store(true, Ordering::Release);
        }
        pending.insert(token.clone(), work.clone());
    }
    let directory = app
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?
        .join("record-imports")
        .join(&token);
    let result = (|| {
        check(&app, epoch, &work)?;
        fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let archive = archive::read_zip(std::path::Path::new(&path), &directory, &|| {
            check(&app, epoch, &work)
        })?;
        let preview = app.state::<Db>().preview_archive(&archive)?;
        check(&app, epoch, &work)?;
        let response = PreparedPreview {
            token: token.clone(),
            preview: preview.clone(),
        };
        *work.prepared.lock().map_err(|_| "Import unavailable")? = Some(Prepared {
            archive,
            preview,
            directory: directory.clone(),
            epoch,
        });
        Ok(response)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(directory);
        app.state::<Imports>().0.lock().unwrap().remove(&token);
    }
    result
}
#[tauri::command(async)]
pub fn restore_record_archive(app: tauri::AppHandle, token: String) -> Result<Preview, String> {
    let work = app
        .state::<Imports>()
        .0
        .lock()
        .map_err(|_| "Import unavailable")?
        .get(&token)
        .cloned()
        .ok_or("Choose the archive again.")?;
    let prepared = work
        .prepared
        .lock()
        .map_err(|_| "Import unavailable")?
        .take()
        .ok_or("The archive is not ready.")?;
    let result = (|| {
        check(&app, prepared.epoch, &work)?;
        let data = app.path().app_data_dir().map_err(|e| e.to_string())?;
        app.state::<Db>().restore_archive(
            &prepared.archive,
            &prepared.preview.fingerprint,
            &prepared.directory,
            &crate::audio_dir(&app)?,
            &data.join("recovery"),
            &|| check(&app, prepared.epoch, &work),
        )
    })();
    app.state::<Imports>().0.lock().unwrap().remove(&token);
    result
}
// Cancellation must also work after device protection has closed the Record.
#[tauri::command]
pub fn discard_record_restore(imports: tauri::State<Imports>, token: String) {
    if let Some(work) = imports.0.lock().unwrap().remove(&token) {
        work.cancelled.store(true, Ordering::Release);
    }
}
