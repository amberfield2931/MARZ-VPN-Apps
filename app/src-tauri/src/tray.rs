//! The icon next to the clock: shows the VPN at a glance, reopens the window, connects and disconnects.
//! Linux trays have no click event through AppIndicator, so everything is in the menu.
use marz_core::controller::{Controller, QuitStart, View};
use std::sync::Arc;
use std::time::Duration;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

const ON: &[u8] = include_bytes!("../icons/tray-on.png");
const OFF: &[u8] = include_bytes!("../icons/tray-off.png");
const WARN: &[u8] = include_bytes!("../icons/tray-warn.png");

pub fn show_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
    if let Some(c) = app.try_state::<Arc<Controller>>() {
        c.on_visible(true);
    }
}

fn show_on_main(app: &AppHandle) {
    let shown = app.clone();
    let _ = app.run_on_main_thread(move || show_window(&shown));
}

/// Quit in the tray. With everything off the app closes at once; otherwise the window opens with the question or the
/// error, and nothing has been changed. The tray thread never waits on the service.
fn request_quit(app: &AppHandle) {
    let Some(c) = app.try_state::<Arc<Controller>>().map(|c| c.inner().clone()) else { return };
    let app = app.clone();
    std::thread::spawn(move || match c.begin_quit() {
        QuitStart::Close => exit(&app),
        QuitStart::Show => show_on_main(&app),
        QuitStart::Busy => {}
    });
}

/// The user said yes, or "try again": the big button's own Stop, and the app closes only when the service confirms the
/// network is back. If it cannot, the window shows the error.
pub fn disconnect_and_exit(app: &AppHandle, c: Arc<Controller>) {
    if !c.quit_waits_for_user() {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || match c.disconnect_for_quit() {
        Some(Ok(())) => exit(&app),
        Some(Err(_)) => show_on_main(&app),
        None => {}
    });
}

fn exit(app: &AppHandle) {
    let closing = app.clone();
    let _ = app.run_on_main_thread(move || closing.exit(0));
}

/// What the tray shows for a state. Same meaning as the big button.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Look {
    Off,
    On,
    Warn,
}

pub fn look(v: &View) -> Look {
    let no_data = v.engine == "connected" && (v.traffic == "lost" || v.traffic == "no_network");
    // Connecting normally is not a warning; only a block that is waiting on a failure is.
    let blocked = v.kill_switch == "armed" && v.engine != "connected" && (v.restarts > 0 || v.engine_error.is_some() || !v.config_ready);
    if no_data || blocked || !v.helper_ok {
        Look::Warn
    } else if v.engine == "connected" || (v.engine == "connecting" && v.restarts > 0) {
        Look::On
    } else {
        Look::Off
    }
}

fn language(v: &View) -> String {
    if v.prefs.language != "system" {
        return v.prefs.language.clone();
    }
    let l = std::env::var("LC_ALL").or_else(|_| std::env::var("LC_MESSAGES")).or_else(|_| std::env::var("LANG")).unwrap_or_default().to_lowercase();
    match l.get(..2) {
        Some("ru") => "ru",
        Some("fa") => "fa",
        Some("zh") => "zh-CN",
        _ => "en",
    }
    .to_string()
}

/// (open, connect, disconnect, quit) from the same words as the app.
fn words(lang: &str) -> (&'static str, &'static str, &'static str, &'static str) {
    match lang {
        "ru" => ("Открыть MARZ VPN", "Подключить", "Отключить", "Выйти"),
        "fa" => ("باز کردن MARZ VPN", "اتصال", "قطع اتصال", "خروج"),
        "zh-CN" => ("打开 MARZ VPN", "连接", "断开连接", "退出"),
        _ => ("Open MARZ VPN", "Connect", "Disconnect", "Quit"),
    }
}

fn toggle_text(v: &View, w: (&'static str, &'static str, &'static str, &'static str)) -> &'static str {
    if v.engine == "connected" || v.engine == "connecting" {
        w.2
    } else {
        w.1
    }
}

fn can_toggle(v: &View) -> bool {
    v.helper_ok && v.session != "other" && (matches!(v.engine.as_str(), "connected" | "connecting") || (v.engine == "disconnected" && v.config_ready))
}

pub fn install(app: &AppHandle, controller: Arc<Controller>) -> tauri::Result<()> {
    let v = controller.view();
    let w = words(&language(&v));
    let open = MenuItem::with_id(app, "open", w.0, true, None::<&str>)?;
    let toggle = MenuItem::with_id(app, "toggle", toggle_text(&v, w), can_toggle(&v), None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", w.3, true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &toggle, &PredefinedMenuItem::separator(app)?, &quit])?;
    let c = controller.clone();
    let tray = TrayIconBuilder::with_id("main")
        .icon(Image::from_bytes(OFF)?)
        .tooltip("MARZ VPN")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "open" => show_window(app),
            "toggle" => {
                let c = c.clone();
                std::thread::spawn(move || c.toggle());
            }
            "quit" => request_quit(app),
            _ => {}
        })
        .on_tray_icon_event(|icon, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_window(icon.app_handle());
            }
        })
        .build(app)?;
    // The tray objects must be used on the main thread; state is compared here and applied there.
    let handle = app.clone();
    std::thread::spawn(move || {
        let mut shown: Option<(Look, String, bool, String)> = None;
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let v = controller.view();
            let lang = language(&v);
            let w = words(&lang);
            let now = (look(&v), toggle_text(&v, w).to_string(), can_toggle(&v), lang);
            if shown.as_ref() == Some(&now) {
                continue;
            }
            let (open, toggle, quit, tray) = (open.clone(), toggle.clone(), quit.clone(), tray.clone());
            let next = now.clone();
            let _ = handle.run_on_main_thread(move || {
                let bytes = match next.0 {
                    Look::On => ON,
                    Look::Off => OFF,
                    Look::Warn => WARN,
                };
                if let Ok(image) = Image::from_bytes(bytes) {
                    let _ = tray.set_icon(Some(image));
                }
                let _ = open.set_text(w.0);
                let _ = toggle.set_text(next.1);
                let _ = toggle.set_enabled(next.2);
                let _ = quit.set_text(w.3);
            });
            shown = Some(now);
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_has_all_words() {
        for l in ["ru", "en", "fa", "zh-CN"] {
            let w = words(l);
            assert!(!w.0.is_empty() && !w.1.is_empty() && !w.2.is_empty() && !w.3.is_empty());
        }
    }
}
