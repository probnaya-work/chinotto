use tauri::{AppHandle, Manager, WebviewWindowBuilder};

#[cfg(all(target_os = "macos", feature = "mas"))]
pub const APP_MENU_OPEN: &str = "chinotto-app-open";
#[cfg(all(target_os = "macos", feature = "mas"))]
pub const WINDOW_MENU_OPEN: &str = "chinotto-window-open";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Distribution {
    Direct,
    Mas,
}

pub const fn distribution() -> Distribution {
    if cfg!(feature = "mas") {
        Distribution::Mas
    } else {
        Distribution::Direct
    }
}

#[cfg(test)]
pub const fn installs_standard_reopen_menu(distribution: Distribution) -> bool {
    matches!(distribution, Distribution::Mas)
}

pub const fn tray_labels(distribution: Distribution) -> (&'static str, &'static str) {
    match distribution {
        Distribution::Direct => ("open chinotto", "quit"),
        Distribution::Mas => ("Open Chinotto", "Quit Chinotto"),
    }
}

trait MainWindowRuntime {
    fn main_window_exists(&self) -> bool;
    fn create_main_window(&mut self) -> bool;
    fn unminimize_main_window(&mut self);
    fn show_main_window(&mut self);
    fn activate_application(&mut self);
    fn focus_main_window(&mut self);
    fn hide_main_window(&mut self);
    fn exit_application(&mut self);
}

fn reopen<R: MainWindowRuntime>(runtime: &mut R) {
    if !runtime.main_window_exists() && !runtime.create_main_window() {
        return;
    }
    runtime.unminimize_main_window();
    runtime.show_main_window();
    runtime.activate_application();
    runtime.focus_main_window();
}

fn close<R: MainWindowRuntime>(runtime: &mut R) {
    runtime.hide_main_window();
}

fn quit<R: MainWindowRuntime>(runtime: &mut R) {
    runtime.exit_application();
}

struct TauriMainWindowRuntime<'a> {
    app: &'a AppHandle,
}

impl TauriMainWindowRuntime<'_> {
    fn with_main_window(&self, action: impl FnOnce(tauri::WebviewWindow)) {
        if let Some(window) = self.app.get_webview_window("main") {
            action(window);
        }
    }
}

impl MainWindowRuntime for TauriMainWindowRuntime<'_> {
    fn main_window_exists(&self) -> bool {
        self.app.get_webview_window("main").is_some()
    }

    fn create_main_window(&mut self) -> bool {
        let Some(config) = self
            .app
            .config()
            .app
            .windows
            .iter()
            .find(|window| window.label == "main")
        else {
            log::warn!("reopen main window: missing main window config");
            return false;
        };

        match WebviewWindowBuilder::from_config(self.app, config)
            .and_then(|builder| builder.build())
        {
            Ok(_) => true,
            Err(error) => {
                log::warn!("reopen main window: failed to recreate window: {error}");
                false
            }
        }
    }

    fn unminimize_main_window(&mut self) {
        self.with_main_window(|window| {
            if let Err(error) = window.unminimize() {
                log::warn!("reopen main window: failed to unminimize: {error}");
            }
        });
    }

    fn show_main_window(&mut self) {
        self.with_main_window(|window| {
            if let Err(error) = window.show() {
                log::warn!("reopen main window: failed to show: {error}");
            }
        });
    }

    fn activate_application(&mut self) {
        #[cfg(target_os = "macos")]
        {
            use objc2::MainThreadMarker;
            use objc2_app_kit::NSApplication;
            use objc2_foundation::NSProcessInfo;

            let Some(main_thread) = MainThreadMarker::new() else {
                log::warn!("reopen main window: application activation was not on the main thread");
                return;
            };
            let application = NSApplication::sharedApplication(main_thread);
            if NSProcessInfo::processInfo()
                .operatingSystemVersion()
                .majorVersion
                >= 14
            {
                application.activate();
            } else {
                #[allow(deprecated)]
                application.activateIgnoringOtherApps(true);
            }
        }
    }

    fn focus_main_window(&mut self) {
        self.with_main_window(|window| {
            if let Err(error) = window.set_focus() {
                log::warn!("reopen main window: failed to focus: {error}");
            }
        });
    }

    fn hide_main_window(&mut self) {
        self.with_main_window(|window| {
            if let Err(error) = window.hide() {
                log::warn!("close main window: failed to hide: {error}");
            }
        });
    }

    fn exit_application(&mut self) {
        self.app.exit(0);
    }
}

pub fn reopen_main_window(app: &AppHandle) {
    reopen(&mut TauriMainWindowRuntime { app });
}

pub fn hide_main_window(app: &AppHandle) {
    close(&mut TauriMainWindowRuntime { app });
}

pub fn quit_application(app: &AppHandle) {
    quit(&mut TauriMainWindowRuntime { app });
}

#[cfg(all(target_os = "macos", feature = "mas"))]
pub fn mas_menu(app: &AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, WINDOW_SUBMENU_ID};

    let menu = Menu::default(app)?;
    let application_menu = menu
        .items()?
        .into_iter()
        .next()
        .and_then(|item| item.as_submenu().cloned())
        .ok_or_else(|| tauri::Error::AssetNotFound("macOS application menu".into()))?;
    let window_menu = menu
        .get(WINDOW_SUBMENU_ID)
        .and_then(|item| item.as_submenu().cloned())
        .ok_or_else(|| tauri::Error::AssetNotFound("macOS Window menu".into()))?;

    let app_open = MenuItem::with_id(app, APP_MENU_OPEN, "Open Chinotto", true, None::<&str>)?;
    let app_separator = PredefinedMenuItem::separator(app)?;
    application_menu.insert_items(&[&app_open, &app_separator], 2)?;

    let window_open =
        MenuItem::with_id(app, WINDOW_MENU_OPEN, "Open Chinotto", true, None::<&str>)?;
    let window_separator = PredefinedMenuItem::separator(app)?;
    window_menu.prepend_items(&[&window_open, &window_separator])?;

    Ok(menu)
}

#[cfg(all(target_os = "macos", feature = "mas"))]
pub fn on_mas_menu_event(app: &AppHandle, event: tauri::menu::MenuEvent) {
    if matches!(event.id().as_ref(), APP_MENU_OPEN | WINDOW_MENU_OPEN) {
        reopen_main_window(app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakeRuntime {
        running: bool,
        window_exists: bool,
        visible: bool,
        minimized: bool,
        create_count: usize,
        activation_count: usize,
        focus_count: usize,
    }

    impl MainWindowRuntime for FakeRuntime {
        fn main_window_exists(&self) -> bool {
            self.window_exists
        }

        fn create_main_window(&mut self) -> bool {
            self.create_count += 1;
            self.window_exists = true;
            true
        }

        fn unminimize_main_window(&mut self) {
            self.minimized = false;
        }

        fn show_main_window(&mut self) {
            self.visible = true;
        }

        fn activate_application(&mut self) {
            self.activation_count += 1;
        }

        fn focus_main_window(&mut self) {
            self.focus_count += 1;
        }

        fn hide_main_window(&mut self) {
            self.visible = false;
        }

        fn exit_application(&mut self) {
            self.running = false;
        }
    }

    fn running_window() -> FakeRuntime {
        FakeRuntime {
            running: true,
            window_exists: true,
            visible: true,
            ..Default::default()
        }
    }

    #[test]
    fn close_hides_the_main_window_without_destroying_or_quitting() {
        let mut runtime = running_window();

        close(&mut runtime);

        assert!(runtime.running);
        assert!(runtime.window_exists);
        assert!(!runtime.visible);
    }

    #[test]
    fn menu_reopen_shows_activates_and_focuses_the_existing_window() {
        let mut runtime = running_window();
        runtime.visible = false;
        runtime.minimized = true;

        reopen(&mut runtime);

        assert_eq!(runtime.create_count, 0);
        assert!(!runtime.minimized);
        assert!(runtime.visible);
        assert_eq!(runtime.activation_count, 1);
        assert_eq!(runtime.focus_count, 1);
    }

    #[test]
    fn tray_reopen_recreates_a_missing_window() {
        let mut runtime = FakeRuntime {
            running: true,
            ..Default::default()
        };

        reopen(&mut runtime);

        assert!(runtime.window_exists);
        assert_eq!(runtime.create_count, 1);
        assert!(runtime.visible);
        assert_eq!(runtime.activation_count, 1);
        assert_eq!(runtime.focus_count, 1);
    }

    #[test]
    fn repeated_close_and_reopen_never_creates_a_duplicate_window() {
        let mut runtime = running_window();

        for _ in 0..5 {
            close(&mut runtime);
            reopen(&mut runtime);
        }

        assert_eq!(runtime.create_count, 0);
        assert!(runtime.window_exists);
        assert!(runtime.visible);
        assert_eq!(runtime.activation_count, 5);
        assert_eq!(runtime.focus_count, 5);
    }

    #[test]
    fn explicit_quit_terminates_the_application() {
        let mut runtime = running_window();

        quit(&mut runtime);

        assert!(!runtime.running);
    }

    #[test]
    fn reopen_menu_and_tray_labels_are_mas_only_changes() {
        assert!(installs_standard_reopen_menu(Distribution::Mas));
        assert!(!installs_standard_reopen_menu(Distribution::Direct));
        assert_eq!(
            tray_labels(Distribution::Mas),
            ("Open Chinotto", "Quit Chinotto")
        );
        assert_eq!(tray_labels(Distribution::Direct), ("open chinotto", "quit"));
    }
}
