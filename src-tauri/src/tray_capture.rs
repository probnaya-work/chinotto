//! Menu bar / system tray: open a minimal capture popover (desktop only).

#[cfg(target_os = "linux")]
use tauri::menu::Menu;
use tauri::{
    image::Image,
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    App, LogicalSize, Manager, PhysicalPosition, Position, Rect, Size, WebviewWindow,
};

/// Below this the panel cannot have drawn itself, so a bad measurement is ignored rather
/// than collapsing the window onto the caret.
const POPOVER_MIN: f64 = 96.0;

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
/// different appearance, the interface at 85% — and a frame that does not follow costs a
/// clipped line on one axis and a panel sitting off-centre under the glyph on the other.
/// The panel is the only thing that knows its own drawn size, and the window is the only
/// thing that knows where the screen ends, so the measurement crosses once, here, and is
/// clamped and re-centred on this side.
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

fn toggle_capture_popover(app: &tauri::AppHandle, tray_rect: &Rect) {
    let Some(window) = app.get_webview_window("capture-popover") else {
        return;
    };

    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        return;
    }

    if let Some(pos) = physical_position_below_tray(tray_rect, &window) {
        let _ = window.set_position(pos);
    }

    let _ = window.show();
    let _ = window.set_focus();
}

/// Registers the tray icon and wires left-click to the capture popover window.
///
/// On macOS, Tauri’s tray uses `NSStatusBar` / `NSStatusItem`; [`TrayIconBuilder::icon_as_template`]
/// sets `NSImage.template` so the system applies light/dark tinting (same behavior as Wi-Fi, battery, etc.).
pub fn setup(app: &App) -> tauri::Result<()> {
    #[cfg(target_os = "linux")]
    let tray_menu = Menu::new(app)?;

    // The <=20px rung: 17pt inside a 22pt box, pure black on transparency, shipped @2x so
    // it is crisp on retina. `icon_as_template` below is what makes the system tint it with
    // the bar, invert it under a light bar, and turn it white while the popover is open.
    let icon = Image::from_bytes(include_bytes!("../icons/tray_menu_template@2x.png"))
        .expect("icons/tray_menu_template@2x.png must decode for menu bar tray");

    let builder = TrayIconBuilder::with_id("chinotto-tray")
        .tooltip("Chinotto — capture a thought")
        .show_menu_on_left_click(false)
        .icon(icon);

    #[cfg(target_os = "macos")]
    let builder = builder.icon_as_template(true);

    let builder = builder.on_tray_icon_event(|tray, event| {
        let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            rect,
            ..
        } = event
        else {
            return;
        };
        toggle_capture_popover(tray.app_handle(), &rect);
    });

    #[cfg(target_os = "linux")]
    let builder = builder.menu(&tray_menu);

    let _tray = builder.build(app)?;
    Ok(())
}
