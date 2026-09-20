//! Menu bar / system tray: open a minimal capture popover (desktop only).

#[cfg(target_os = "linux")]
use tauri::menu::Menu;
use tauri::{
    image::Image,
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    App, Manager, PhysicalPosition, Position, Rect, Size, WebviewWindow,
};

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
