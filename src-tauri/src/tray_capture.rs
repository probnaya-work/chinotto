//! Menu bar / system tray: the glyph, its one modifier, the capture popover and the
//! secondary-click menu.
//!
//! The tray is not a second product. It is the app's edge moved to where the cursor already
//! is: the caret, the voice, and a Return when one is due — never a list, a search field or
//! a settings tree. Anything that wants more room opens the window instead.
//!
//! Everything the menu says is read from the Record at the moment it is refreshed. Nothing
//! here computes a status of its own, and nothing here *decides* a Return happened — see
//! [`crate::db::Db::return_waiting`].

use std::sync::Mutex;

use tauri::{
    image::Image,
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    App, Emitter, LogicalSize, Manager, PhysicalPosition, Position, Rect, Size, WebviewWindow,
};

use crate::db::Db;

/// Below this the panel cannot have drawn itself, so a bad measurement is ignored rather
/// than collapsing the window onto the caret.
const POPOVER_MIN: f64 = 96.0;

const MENU_OPEN: &str = "chinotto-tray-open";
const MENU_TODAY: &str = "chinotto-tray-today";
const MENU_SYNC: &str = "chinotto-tray-sync";
const MENU_SETTINGS: &str = "chinotto-tray-settings";
const MENU_QUIT: &str = "chinotto-tray-quit";

/// The two lines of the menu that state something, and the one fact the Record cannot
/// answer for itself.
///
/// Whether sync is configured is a frontend fact — it is a build-time `VITE_` variable, and
/// `isFirebaseSyncConfigured()` is the single place that reads it — so the webview reports
/// it here rather than Rust inventing a second opinion about it. Until it has, the line
/// says nothing rather than guessing.
struct TrayMenu {
    today: MenuItem<tauri::Wry>,
    sync: MenuItem<tauri::Wry>,
    sync_on: Mutex<Option<bool>>,
    /// Which glyph is currently on the bar, so it is only redrawn when it changes.
    waiting: Mutex<Option<bool>>,
}

fn to_i32(v: impl Into<f64>) -> i32 {
    v.into().round() as i32
}

fn physical_position_below_tray(tray_rect: &Rect, window: &WebviewWindow) -> Option<PhysicalPosition<i32>> {
    let scale = window
        .current_monitor()
        .ok()
        .flatten()
        .map(|m| m.scale_factor())
        .unwrap_or(1.0);

    let (px, py, pw, ph) = match (&tray_rect.position, &tray_rect.size) {
        (Position::Physical(p), Size::Physical(s)) => (
            to_i32(p.x),
            to_i32(p.y),
            to_i32(s.width),
            to_i32(s.height),
        ),
        (Position::Logical(p), Size::Logical(s)) => (
            to_i32(p.x * scale),
            to_i32(p.y * scale),
            to_i32(s.width * scale),
            to_i32(s.height * scale),
        ),
        (Position::Physical(p), Size::Logical(s)) => (
            to_i32(p.x),
            to_i32(p.y),
            to_i32(s.width * scale),
            to_i32(s.height * scale),
        ),
        (Position::Logical(p), Size::Physical(s)) => (
            to_i32(p.x * scale),
            to_i32(p.y * scale),
            to_i32(s.width),
            to_i32(s.height),
        ),
    };

    let outer = window.outer_size().ok()?;
    let x = px + pw / 2 - (outer.width as i32) / 2;
    let y = py + ph;
    Some(PhysicalPosition::new(x, y))
}

/// How much of the screen a popover hanging off the menu bar may take before it would run
/// past the bottom of the display. The panel grows downward from a fixed top edge, so this is
/// the only bound it has.
fn max_popover_height(window: &WebviewWindow) -> f64 {
    let Some(monitor) = window.current_monitor().ok().flatten() else {
        return f64::MAX;
    };
    let scale = monitor.scale_factor();
    let monitor_bottom = (monitor.position().y as f64 + monitor.size().height as f64) / scale;
    let top = window
        .outer_position()
        .map(|p| p.y as f64 / scale)
        .unwrap_or(0.0);
    // A hair of room so the shadow is never flush against the screen's edge.
    (monitor_bottom - top - 8.0).max(POPOVER_MIN)
}

/// The panel measures itself and says how big it has become; the window becomes that.
///
/// The window is a frame around a panel that changes size — a second line of words, a
/// Return above the caret, the waveform, the interface at 85% — and a frame that does not
/// follow costs a clipped line on one axis and a panel sitting off-centre under the glyph
/// on the other. The panel is the only thing that knows its own drawn size, and the window
/// is the only thing that knows where the screen ends, so the measurement crosses once,
/// here, and is clamped and re-centred on this side.
///
/// `set_size` holds the top-left corner, so the width has to be paid for on both sides or
/// the panel walks left of the glyph every time it changes.
#[tauri::command]
pub fn fit_capture_popover(app: tauri::AppHandle, width: f64, height: f64) -> Result<(), String> {
    let Some(window) = app.get_webview_window("capture-popover") else {
        return Ok(());
    };
    if !width.is_finite() || !height.is_finite() || width < POPOVER_MIN || height < POPOVER_MIN {
        return Ok(());
    }
    let height = height.min(max_popover_height(&window));

    let scale = window.scale_factor().unwrap_or(1.0);
    let before = window.outer_size().map(|s| s.width as f64 / scale).ok();

    window
        .set_size(LogicalSize::new(width, height))
        .map_err(|e| e.to_string())?;

    if let (Some(before), Ok(pos)) = (before, window.outer_position()) {
        let dx = ((before - width) / 2.0 * scale).round() as i32;
        if dx != 0 {
            let _ = window.set_position(PhysicalPosition::new(pos.x + dx, pos.y));
        }
    }
    Ok(())
}

/// The start and end of the local day, as the RFC3339 instants the Record stores.
///
/// "Today" is the day the person is having, not a UTC window: something left at 23:10 is
/// today's until they go to bed, and at 00:10 the count starts again. Computed here rather
/// than in the webview because the menu has to be right while no window is open.
fn local_day_bounds() -> (String, String) {
    use chrono::{Duration, Local, TimeZone};
    let today = Local::now().date_naive();
    let start = Local
        .from_local_datetime(&today.and_hms_opt(0, 0, 0).expect("midnight exists"))
        .earliest()
        .map(|d| d.to_utc())
        .unwrap_or_else(chrono::Utc::now);
    (start.to_rfc3339(), (start + Duration::days(1)).to_rfc3339())
}

/// Re-reads everything the menu bar states, and re-draws the glyph if its modifier changed.
///
/// Cheap by construction — two counts and no list — because it runs on every popover toggle
/// and after every write. Tauri proxies tray events through the event loop, so a right-click
/// handler runs *after* AppKit has already popped the menu; the labels are therefore kept
/// current as the state changes rather than rebuilt when the menu opens.
pub fn refresh(app: &tauri::AppHandle) {
    let Some(state) = app.try_state::<TrayMenu>() else {
        return;
    };
    let Some(db) = app.try_state::<Db>() else {
        return;
    };

    let (from, to) = local_day_bounds();
    if let Ok(count) = db.count_between(&from, &to) {
        let _ = state.today.set_text(format!("today · {count}"));
    }

    let sync_on = state.sync_on.lock().ok().and_then(|v| *v);
    let _ = state.sync.set_text(match sync_on {
        Some(true) => "sync on",
        Some(false) => "sync off",
        None => "sync",
    });

    let waiting = db.return_waiting().unwrap_or(false);
    let changed = state
        .waiting
        .lock()
        .map(|mut held| {
            let changed = *held != Some(waiting);
            *held = Some(waiting);
            changed
        })
        .unwrap_or(true);
    if changed {
        draw_glyph(app, waiting);
    }
}

/// Puts a glyph on the bar, and says again that it is a template.
///
/// The second half is not optional and is the reason this is one function. `set_icon` hands
/// the status item a fresh `NSImage`, and `NSImage.template` is a property of the image, not
/// of the item — tray-icon sets it to `false` on the way through. An icon set without this
/// line is a mask drawn as artwork: pure black on a dark menu bar, beside everybody else's
/// white, and it stops inverting under a light bar.
fn draw_glyph(app: &tauri::AppHandle, waiting: bool) {
    let Some(tray) = app.tray_by_id("chinotto-tray") else {
        return;
    };
    let _ = tray.set_icon(Some(glyph(waiting)));
    #[cfg(target_os = "macos")]
    let _ = tray.set_icon_as_template(true);
    let _ = tray.set_tooltip(Some(if waiting {
        "Chinotto — a return is waiting"
    } else {
        "Chinotto — capture a thought"
    }));
}

/// The webview reports what only it knows, and the menu bar catches up.
///
/// Called after every write the Record makes, from both surfaces, so `today · n` is right
/// the moment a fragment lands rather than the next time the panel is opened.
#[tauri::command]
pub fn refresh_tray(app: tauri::AppHandle, sync_on: Option<bool>) -> Result<(), String> {
    if let (Some(value), Some(state)) = (sync_on, app.try_state::<TrayMenu>()) {
        if let Ok(mut held) = state.sync_on.lock() {
            *held = Some(value);
        }
    }
    refresh(&app);
    Ok(())
}

/// The <=20px rung, with or without its one modifier.
///
/// Both are real macOS template images: pure black on transparency, so the system tints
/// them with the bar, inverts them under a light bar, and turns them white while the
/// popover is open. Shipped @2x so they are crisp on retina.
fn glyph(waiting: bool) -> Image<'static> {
    let bytes: &[u8] = if waiting {
        include_bytes!("../icons/tray_menu_waiting_template@2x.png")
    } else {
        include_bytes!("../icons/tray_menu_template@2x.png")
    };
    Image::from_bytes(bytes).expect("menu bar glyph must decode")
}

/// Puts the panel under the glyph and gives it the caret.
///
/// `tray_rect` is the glyph's own frame. A click brings it along; anything else — settings'
/// "try it" — asks the tray icon for it, so the panel arrives in the same place either way
/// rather than wherever the window happened to be left.
pub fn show_capture_popover(app: &tauri::AppHandle, tray_rect: Option<Rect>) {
    let Some(window) = app.get_webview_window("capture-popover") else {
        return;
    };

    let rect = tray_rect.or_else(|| app.tray_by_id("chinotto-tray").and_then(|t| t.rect().ok().flatten()));
    if let Some(pos) = rect.and_then(|r| physical_position_below_tray(&r, &window)) {
        let _ = window.set_position(pos);
    }

    // The panel asks the Record what to draw as it opens, so it has to be told that it is
    // opening: the webview is never torn down between one open and the next.
    let _ = window.emit("chinotto-tray-opened", ());
    let _ = window.show();
    let _ = window.set_focus();
}

fn toggle_capture_popover(app: &tauri::AppHandle, tray_rect: &Rect) {
    let Some(window) = app.get_webview_window("capture-popover") else {
        return;
    };

    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        return;
    }

    show_capture_popover(app, Some(tray_rect.clone()));
}

/// Whether the capture popover is on screen.
///
/// The voice chord uses this to decide which surface is speaking, so that holding it over
/// the panel does not haul the main window out in front of what you were looking at.
pub fn popover_is_open(app: &tauri::AppHandle) -> bool {
    app.get_webview_window("capture-popover")
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false)
}

fn on_menu(app: &tauri::AppHandle, event: MenuEvent) {
    match event.id().as_ref() {
        MENU_OPEN => crate::ensure_main_window_focus(app),
        MENU_SETTINGS => {
            crate::ensure_main_window_focus(app);
            let _ = app.emit("chinotto-open-settings", ());
        }
        MENU_QUIT => app.exit(0),
        // `today · n` and the sync line state something; they are not verbs and are built
        // disabled, so they never arrive here.
        _ => {}
    }
}

/// Registers the tray icon, its menu, and wires left-click to the capture popover window.
///
/// On macOS, Tauri’s tray uses `NSStatusBar` / `NSStatusItem`; [`TrayIconBuilder::icon_as_template`]
/// sets `NSImage.template` so the system applies light/dark tinting (same behavior as Wi-Fi, battery, etc.).
///
/// The menu carries no accelerators. The identity file draws `⌘⇧C` and `⌘,` beside two of
/// the lines, but this app has no menu bar of its own: `⌘⇧K` belongs to the global-shortcut
/// plugin and `⌘,` is handled inside the webview, and declaring either as an `NSMenuItem`
/// key equivalent would take it away from its owner while the app is frontmost. The keys
/// keep working; the menu does not claim them.
pub fn setup(app: &App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, MENU_OPEN, "open chinotto", true, None::<&str>)?;
    // Both of these are statements rather than verbs, so they are built disabled: on macOS
    // that is what a menu line that reports something looks like, and it is also what stops
    // somebody clicking a number.
    let today = MenuItem::with_id(app, MENU_TODAY, "today", false, None::<&str>)?;
    let sync = MenuItem::with_id(app, MENU_SYNC, "sync", false, None::<&str>)?;
    let settings = MenuItem::with_id(app, MENU_SETTINGS, "settings", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, MENU_QUIT, "quit", true, None::<&str>)?;

    let tray_menu = Menu::with_items(
        app,
        &[
            &open,
            &today,
            &PredefinedMenuItem::separator(app)?,
            &sync,
            &settings,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    app.manage(TrayMenu {
        today,
        sync,
        sync_on: Mutex::new(None),
        // The builder below draws the resting glyph, so that is what is already on the bar.
        waiting: Mutex::new(Some(false)),
    });

    let builder = TrayIconBuilder::with_id("chinotto-tray")
        .tooltip("Chinotto — capture a thought")
        // Left click is the capture panel; the menu is the secondary click, as it is for
        // every other status item on the bar.
        .show_menu_on_left_click(false)
        .menu(&tray_menu)
        .icon(glyph(false));

    #[cfg(target_os = "macos")]
    let builder = builder.icon_as_template(true);

    let builder = builder
        .on_menu_event(on_menu)
        .on_tray_icon_event(|tray, event| {
            let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = event
            else {
                return;
            };
            let app = tray.app_handle();
            toggle_capture_popover(app, &rect);
            refresh(app);
        });

    let _tray = builder.build(app)?;
    refresh(app.app_handle());
    Ok(())
}
