//! App-wide, device-local read permission. No biometric data enters Chinotto.
use serde::Serialize;
use std::{path::PathBuf, sync::Mutex};
use tauri::{Emitter, Manager};

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Snapshot {
    pub enabled: bool,
    pub locked: bool,
    pub epoch: u64,
    pub busy: bool,
}
struct Session {
    snapshot: Snapshot,
    path: PathBuf,
}
pub struct Privacy(Mutex<Session>);

impl Privacy {
    fn load(path: PathBuf) -> Self {
        // An unreadable existing setting must never silently turn protection off.
        let enabled = match std::fs::read_to_string(&path) {
            Ok(value) => value.trim() != "off",
            Err(error) => error.kind() != std::io::ErrorKind::NotFound,
        };
        Self(Mutex::new(Session {
            path,
            snapshot: Snapshot {
                enabled,
                locked: enabled,
                epoch: 0,
                busy: false,
            },
        }))
    }
    pub fn snapshot(&self) -> Result<Snapshot, String> {
        self.0
            .lock()
            .map(|s| s.snapshot)
            .map_err(|_| "device protection unavailable".into())
    }
    fn lock(&self) {
        if let Ok(mut session) = self.0.lock() {
            if session.snapshot.enabled || session.snapshot.busy {
                session.snapshot.epoch += 1;
            }
            session.snapshot.locked = session.snapshot.enabled;
        }
    }
    fn begin(&self) -> Result<u64, String> {
        let mut session = self.0.lock().map_err(|_| "device protection unavailable")?;
        if session.snapshot.busy {
            return Err("authentication already in progress".into());
        }
        session.snapshot.busy = true;
        Ok(session.snapshot.epoch)
    }
    fn finish(&self, action: &str, epoch: u64, authenticated: bool) -> Result<Snapshot, String> {
        let mut session = self.0.lock().map_err(|_| "device protection unavailable")?;
        session.snapshot.busy = false;
        if !authenticated || session.snapshot.epoch != epoch {
            return Err("authentication cancelled or expired".into());
        }
        match action {
            "enable" | "disable" => {
                let enabled = action == "enable";
                // Atomic preference replacement; failed persistence retains the old choice.
                let temporary = session.path.with_extension("tmp");
                std::fs::write(&temporary, if enabled { "on" } else { "off" })
                    .map_err(|e| e.to_string())?;
                std::fs::rename(&temporary, &session.path).map_err(|e| e.to_string())?;
                if session.snapshot.enabled != enabled {
                    session.snapshot.epoch += 1;
                }
                session.snapshot.enabled = enabled;
            }
            "unlock" => {}
            _ => return Err("unknown protection action".into()),
        }
        session.snapshot.locked = false;
        Ok(session.snapshot)
    }
}

pub fn can_read(app: &tauri::AppHandle) -> bool {
    app.try_state::<Privacy>()
        .and_then(|p| p.snapshot().ok())
        .map(|s| !s.locked)
        .unwrap_or(false)
}

/// An export remains temporary until its originating read session is still valid.
pub struct ProtectedExport {
    pub temporary: PathBuf,
    destination: PathBuf,
    epoch: u64,
}
impl ProtectedExport {
    pub fn new(app: &tauri::AppHandle, destination: PathBuf) -> Result<Self, String> {
        let state = app.state::<Privacy>().snapshot()?;
        if state.locked {
            return Err("record_locked".into());
        }
        let temporary = destination.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        Ok(Self {
            temporary,
            destination,
            epoch: state.epoch,
        })
    }
    pub fn check(&self, app: &tauri::AppHandle) -> Result<(), String> {
        let state = app.state::<Privacy>().snapshot()?;
        if state.locked || state.epoch != self.epoch {
            Err("record_locked".into())
        } else {
            Ok(())
        }
    }
    pub fn commit(&self, app: &tauri::AppHandle) -> Result<(), String> {
        let privacy = app.state::<Privacy>();
        let session = privacy
            .0
            .lock()
            .map_err(|_| "device protection unavailable")?;
        if session.snapshot.locked || session.snapshot.epoch != self.epoch {
            return Err("record_locked".into());
        }
        std::fs::rename(&self.temporary, &self.destination).map_err(|e| e.to_string())
    }
}
impl Drop for ProtectedExport {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.temporary);
    }
}

// Default deny: legacy commands and future read surfaces cannot bypass the boundary.
pub fn allowed_while_locked(command: &str) -> bool {
    matches!(
        command,
        "privacy_state"
            | "privacy_authenticate"
            | "privacy_rendered"
            | "capture_fragment"
            | "capture_encounter"
            | "capture_voice"
            | "record_transcript"
            | "create_entry"
            | "start_voice_capture"
            | "stop_voice_capture"
            | "discard_voice_recording"
            | "voice_capture_permission_status"
            | "request_voice_capture_permissions"
            | "local_recognition_status"
            | "prepare_voice_access"
            | "run_native_speech_recognition"
            | "open_microphone_settings"
            | "open_tray_capture"
            | "fit_capture_popover"
            | "refresh_tray"
            | "set_app_icon"
            | "this_device"
            | "register_this_device"
            | "pending_record_sync_count"
            | "failed_record_sync_count"
            | "max_held"
            | "mirror_pending_fragments"
            | "project_entries_into_record"
            | "absorb_remote_deletes"
            | "apply_pending_record_sync_links"
            | "activate_record_sync_user"
            | "record_sync_error"
            | "hash_record_sync_value"
            | "commit_record_sync"
            | "acknowledge_record_sync"
            | "ingest_firestore_entries"
            | "delete_local_entries_for_sync"
            | "apply_remote_entry_theme"
            | "ingest_remote_user_themes"
            | "apply_remote_user_theme_tombstones"
            | "ensure_record_sync_fragment"
    )
}

pub fn lock_app(app: &tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    if app
        .state::<Privacy>()
        .snapshot()
        .map(|s| s.enabled)
        .unwrap_or(true)
    {
        mac::conceal(app);
    }
    app.state::<Privacy>().lock();
    announce(app);
}

fn announce(app: &tauri::AppHandle) {
    if let Ok(state) = app.state::<Privacy>().snapshot() {
        let _ = app.emit("chinotto-privacy", state);
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    crate::tray_capture::refresh(app);
}

#[tauri::command]
pub fn privacy_state(privacy: tauri::State<Privacy>) -> Result<Snapshot, String> {
    privacy.snapshot()
}

#[tauri::command(async)]
pub fn privacy_authenticate(app: tauri::AppHandle, action: String) -> Result<Snapshot, String> {
    if !matches!(action.as_str(), "unlock" | "enable" | "disable") {
        return Err("unknown protection action".into());
    }
    let epoch = app.state::<Privacy>().begin()?;
    announce(&app);
    let authenticated = authenticate();
    let result = app
        .state::<Privacy>()
        .finish(&action, epoch, authenticated.is_ok());
    announce(&app);
    authenticated?;
    result
}

#[cfg(target_os = "macos")]
fn authenticate() -> Result<(), String> {
    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_foundation::{NSError, NSString};
    use objc2_local_authentication::{LAContext, LAPolicy};
    let (tx, rx) = std::sync::mpsc::channel();
    // SAFETY: LAContext supports evaluation off the main thread. The sendable reply
    // captures only a channel sender and copies the result before the OS callback returns.
    unsafe {
        let context = LAContext::new();
        let policy = LAPolicy::DeviceOwnerAuthentication;
        context
            .canEvaluatePolicy_error(policy)
            .map_err(|e| format!("device authentication unavailable: {}", e.code()))?;
        let reply = RcBlock::new(move |success: Bool, _error: *mut NSError| {
            let _ = tx.send(success.as_bool());
        });
        context.evaluatePolicy_localizedReason_reply(
            policy,
            &NSString::from_str("open your Chinotto Record"),
            &reply,
        );
        // Retain the context until completion; dropping it would cancel the OS prompt.
        let success = rx.recv().map_err(|_| "device authentication interrupted")?;
        if success {
            Ok(())
        } else {
            Err("device authentication cancelled or failed".into())
        }
    }
}
#[cfg(not(target_os = "macos"))]
fn authenticate() -> Result<(), String> {
    Err("device protection is available on macOS".into())
}

#[tauri::command]
pub fn privacy_rendered(
    window: tauri::WebviewWindow,
    epoch: u64,
    locked: bool,
    app: tauri::AppHandle,
) {
    #[cfg(target_os = "macos")]
    {
        let label = window.label().to_string();
        let check = app.clone();
        let _ = app.run_on_main_thread(move || {
            // Recheck on the UI thread: a relock may have happened after this IPC arrived.
            if check
                .state::<Privacy>()
                .snapshot()
                .map(|s| s.epoch == epoch && s.locked == locked)
                .unwrap_or(false)
            {
                mac::reveal(&label, locked);
            }
        });
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (window, epoch, locked, app);
}

pub fn setup(app: &tauri::AppHandle) -> Result<(), String> {
    let root = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    app.manage(Privacy::load(root.join("record-protection")));
    #[cfg(target_os = "macos")]
    mac::observe(app.clone());
    Ok(())
}

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use block2::RcBlock;
    use objc2::{rc::Retained, runtime::ProtocolObject, MainThreadMarker, MainThreadOnly};
    use objc2_app_kit::{
        NSAccessibility, NSApplication, NSApplicationDidBecomeActiveNotification,
        NSApplicationWillResignActiveNotification, NSAutoresizingMaskOptions, NSBox, NSBoxType,
        NSColor, NSWindow, NSWindowWillMiniaturizeNotification,
    };
    use objc2_foundation::{NSNotificationCenter, NSObjectProtocol, NSOperationQueue};
    use std::cell::RefCell;
    thread_local! {
        static COVERS: RefCell<std::collections::HashMap<String, Retained<NSBox>>> = RefCell::new(Default::default());
        static OBSERVERS: RefCell<Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>>> = const { RefCell::new(Vec::new()) };
    }
    pub(super) fn conceal(app: &tauri::AppHandle) {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        for (label, window) in app.webview_windows() {
            let Ok(raw) = window.ns_window() else {
                continue;
            };
            // SAFETY: Tauri owns this live NSWindow; all AppKit work is on the main thread.
            let native = unsafe { &*raw.cast::<NSWindow>() };
            let Some(content) = native.contentView() else {
                continue;
            };
            content.setAccessibilityHidden(true);
            COVERS.with_borrow_mut(|covers| {
                if covers.contains_key(&label) {
                    return;
                }
                let cover = NSBox::initWithFrame(NSBox::alloc(mtm), content.bounds());
                cover.setBoxType(NSBoxType::Custom);
                cover.setBorderWidth(0.0);
                cover.setFillColor(&NSColor::colorWithSRGBRed_green_blue_alpha(
                    20.0 / 255.0,
                    20.0 / 255.0,
                    22.0 / 255.0,
                    1.0,
                ));
                cover.setAutoresizingMask(
                    NSAutoresizingMaskOptions::ViewWidthSizable
                        | NSAutoresizingMaskOptions::ViewHeightSizable,
                );
                content.addSubview(&cover);
                content.displayIfNeeded();
                covers.insert(label, cover);
            });
        }
    }
    pub fn reveal(label: &str, locked: bool) {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        // The tray is intentionally nonactivating; only its already-scrubbed capture UI
        // can be revealed without activating Chinotto.
        if !NSApplication::sharedApplication(mtm).isActive()
            && !(locked && label == "capture-popover")
        {
            return;
        }
        COVERS.with_borrow_mut(|covers| {
            if let Some(cover) = covers.remove(label) {
                // SAFETY: this retained view and its superview are accessed only on the main thread.
                if let Some(content) = unsafe { cover.superview() } {
                    content.setAccessibilityHidden(false);
                }
                cover.removeFromSuperview();
            }
        });
    }
    pub fn observe(app: tauri::AppHandle) {
        let center = NSNotificationCenter::defaultCenter();
        let lost = app.clone();
        let resign = RcBlock::new(move |_| {
            if lost
                .state::<Privacy>()
                .snapshot()
                .map(|s| s.enabled)
                .unwrap_or(true)
            {
                conceal(&lost);
            }
            lost.state::<Privacy>().lock();
            announce(&lost);
        });
        let active = RcBlock::new(move |_| announce(&app));
        // SAFETY: these AppKit notifications are delivered on the main operation queue.
        // Blocks own only Send AppHandles; observer tokens live until process termination.
        unsafe {
            let a = center.addObserverForName_object_queue_usingBlock(
                Some(NSApplicationWillResignActiveNotification),
                None,
                Some(&NSOperationQueue::mainQueue()),
                &resign,
            );
            let b = center.addObserverForName_object_queue_usingBlock(
                Some(NSApplicationDidBecomeActiveNotification),
                None,
                Some(&NSOperationQueue::mainQueue()),
                &active,
            );
            let c = center.addObserverForName_object_queue_usingBlock(
                Some(NSWindowWillMiniaturizeNotification),
                None,
                Some(&NSOperationQueue::mainQueue()),
                &resign,
            );
            OBSERVERS.with_borrow_mut(|tokens| {
                tokens.push(a);
                tokens.push(b);
                tokens.push(c);
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn session(enabled: bool) -> Privacy {
        let path = std::env::temp_dir().join(format!("chinotto-privacy-{}", uuid::Uuid::new_v4()));
        let p = Privacy::load(path);
        p.0.lock().unwrap().snapshot = Snapshot {
            enabled,
            locked: enabled,
            epoch: 0,
            busy: false,
        };
        p
    }
    #[test]
    fn cold_start_defaults_off_but_invalid_preference_fails_closed() {
        let p = session(false);
        let path = p.0.lock().unwrap().path.clone();
        assert!(!Privacy::load(path.clone()).snapshot().unwrap().locked);
        std::fs::write(&path, "damaged").unwrap();
        assert!(Privacy::load(path.clone()).snapshot().unwrap().locked);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn auth_is_single_flight_and_old_success_cannot_unlock() {
        let p = session(true);
        let epoch = p.begin().unwrap();
        assert!(p.begin().is_err());
        p.lock();
        assert!(p.finish("unlock", epoch, true).is_err());
        assert!(p.snapshot().unwrap().locked);
        assert!(!p.snapshot().unwrap().busy);
    }
    #[test]
    fn background_does_not_interrupt_reading_with_protection_off() {
        let p = session(false);
        p.lock();
        assert_eq!(p.snapshot().unwrap().epoch, 0);
        assert!(!p.snapshot().unwrap().locked);
        let epoch = p.begin().unwrap();
        p.lock();
        assert!(p.finish("enable", epoch, true).is_err());
        assert!(!p.snapshot().unwrap().enabled);
    }
    #[test]
    fn cancellations_keep_setting_and_success_persists() {
        let p = session(false);
        let epoch = p.begin().unwrap();
        assert!(p.finish("enable", epoch, false).is_err());
        assert!(!p.snapshot().unwrap().enabled);
        let epoch = p.begin().unwrap();
        p.finish("enable", epoch, true).unwrap();
        let path = p.0.lock().unwrap().path.clone();
        assert!(Privacy::load(path.clone()).snapshot().unwrap().locked);
        let epoch = p.begin().unwrap();
        assert!(p.finish("disable", epoch, false).is_err());
        assert!(p.snapshot().unwrap().enabled);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn interrupted_export_removes_only_its_temporary_copy() {
        let destination =
            std::env::temp_dir().join(format!("chinotto-export-{}", uuid::Uuid::new_v4()));
        let temporary = destination.with_extension("tmp");
        std::fs::write(&destination, "earlier export").unwrap();
        std::fs::write(&temporary, "unfinished private export").unwrap();
        drop(ProtectedExport {
            destination: destination.clone(),
            temporary: temporary.clone(),
            epoch: 0,
        });
        assert!(!temporary.exists());
        assert_eq!(
            std::fs::read_to_string(&destination).unwrap(),
            "earlier export"
        );
        std::fs::remove_file(destination).unwrap();
    }
    #[test]
    fn locked_command_boundary_excludes_all_private_entry_points() {
        for name in [
            "recent_fragments",
            "find_fragments",
            "fragment_history",
            "materials_for",
            "select_return",
            "list_entries",
            "get_entry",
            "search_entries",
            "export_record",
            "export_entries",
            "record_sync_snapshot",
            "open_wording_conflicts",
            "create_backup",
            "write_utf8_file",
            "continue_fragment",
            "correct_fragment",
        ] {
            assert!(!allowed_while_locked(name), "{name}");
        }
        for name in [
            "capture_fragment",
            "capture_encounter",
            "capture_voice",
            "privacy_authenticate",
        ] {
            assert!(allowed_while_locked(name));
        }
    }
}
