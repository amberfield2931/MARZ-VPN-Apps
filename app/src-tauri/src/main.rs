//! The window around marz-core's controller. Runs as the signed-in user, never as root/Administrator;
//! everything privileged is behind the helper's typed protocol.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use marz_core::api::{AppApi, PRODUCTION_URL};
use marz_core::controller::Controller;
use marz_core::helper_client::HelperClient;
use marz_core::http::Endpoint;
use marz_core::vault::{KeySource, Vault};
use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use tauri::{Manager, State, WindowEvent};

mod tray;

type C<'a> = State<'a, Arc<Controller>>;

fn data_dir() -> PathBuf {
    #[cfg(feature = "lab")]
    if let Some(d) = std::env::var_os("MARZ_DATA_DIR") {
        return d.into();
    }
    #[cfg(windows)]
    {
        return PathBuf::from(std::env::var_os("APPDATA").unwrap_or_default()).join("MARZ VPN");
    }
    #[cfg(not(windows))]
    {
        if let Some(d) = std::env::var_os("XDG_DATA_HOME").filter(|d| !d.is_empty()) {
            return PathBuf::from(d).join("marzvpn");
        }
        PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share/marzvpn")
    }
}

#[cfg(feature = "lab")]
fn key_source() -> Box<dyn KeySource> {
    Box::new(marz_core::vault::FixedKey([9; 32]))
}

#[cfg(not(feature = "lab"))]
fn key_source() -> Box<dyn KeySource> {
    Box::new(marz_core::vault::OsKeyring)
}

fn api_endpoint() -> Endpoint {
    #[cfg(feature = "lab")]
    if let Ok(url) = std::env::var("MARZ_API_URL") {
        let config = marz_core::tls::client_config_with(Default::default()).expect("tls config");
        return Endpoint::parse_with(&url, config).expect("lab url");
    }
    Endpoint::parse(PRODUCTION_URL).expect("production url")
}

fn helper_endpoint() -> String {
    #[cfg(feature = "lab")]
    if let Ok(s) = std::env::var("MARZ_HELPER_SOCKET") {
        return s;
    }
    HelperClient::default_endpoint().to_string()
}

fn launch(url: &str) {
    #[cfg(windows)]
    let result = Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).spawn();
    #[cfg(not(windows))]
    let result = Command::new("xdg-open").arg(url).spawn();
    if let Ok(mut child) = result {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

#[tauri::command]
fn get_state(c: C) -> Value {
    c.view_json()
}
#[tauri::command(async)]
fn toggle(c: C) {
    c.toggle();
}
#[tauri::command]
fn select_server(c: C, id: Option<String>) {
    c.select_server(id);
}
#[tauri::command]
fn route_check_start(c: C) {
    c.start_route_check();
}
#[tauri::command]
fn route_check_stop(c: C) {
    c.stop_route_check();
}
#[tauri::command]
fn telegram_start(c: C) {
    c.telegram_start();
}
#[tauri::command]
fn telegram_resume(c: C) {
    c.telegram_resume();
}
#[tauri::command]
fn telegram_cancel(c: C) {
    c.telegram_cancel();
}
#[tauri::command]
fn email_send(c: C, email: String, locale: String) {
    c.email_send(email, locale);
}
#[tauri::command]
fn email_verify(c: C, code: String) {
    c.email_verify(code);
}
#[tauri::command]
fn email_resend(c: C, locale: String) {
    c.email_resend(locale);
}
#[tauri::command]
fn email_reset(c: C) {
    c.email_reset();
}
/// Runs only from the confirmed Settings dialog, when the service is down or the session is another user's.
/// The documented recovery goes through the system's administrator prompt (polkit action
/// com.marzvpn.helper.recover, auth_admin; on Windows, UAC). Fixed path, no shell, no input.
fn recover_elevated() -> bool {
    #[cfg(unix)]
    {
        std::process::Command::new("pkexec").args(["/usr/lib/marzvpn/marzvpn-helper", "--recover"]).status().is_ok_and(|s| s.success())
    }
    #[cfg(windows)]
    {
        let Some(helper) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("marzvpn-helper.exe"))) else { return false };
        // The helper's own exit code, not only whether the prompt opened. A refused UAC prompt is an error too.
        let script = format!(
            "$ErrorActionPreference='Stop'; $p = Start-Process -Verb RunAs -Wait -PassThru -WindowStyle Hidden -FilePath '{}' -ArgumentList '--recover'; exit $p.ExitCode",
            helper.display()
        );
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new("powershell").args(["-NoProfile", "-NonInteractive", "-Command", &script]).creation_flags(CREATE_NO_WINDOW).status().is_ok_and(|s| s.success())
    }
}

#[tauri::command(async)]
fn emergency_off(c: C) -> Value {
    match c.emergency_off() {
        Ok(true) => serde_json::json!({"ok": true}),
        // Success only if the recovery says so and the service agrees: the message must never be false.
        Ok(false) => serde_json::json!({"ok": recover_elevated() && c.block_is_gone()}),
        Err(code) => serde_json::json!({"ok": false, "code": code}),
    }
}
#[tauri::command]
fn set_kill_switch(c: C, on: bool) -> bool {
    c.set_kill_switch(on)
}
#[tauri::command]
fn logout(c: C) {
    c.logout();
}
#[tauri::command]
fn mark_welcome_seen(c: C) {
    c.mark_welcome_seen();
}
#[tauri::command]
fn set_theme(c: C, theme: String) {
    c.set_theme(&theme);
}
#[tauri::command]
fn set_language(c: C, language: String) {
    c.set_language(&language);
}
#[tauri::command]
fn on_visible(c: C, visible: bool) {
    c.on_visible(visible);
}
#[tauri::command(async)]
fn checkout_start(c: C, plan: String, action: String, profile: Option<String>, language: String, channel: Option<String>) {
    // The controller validated the link's exact shape; it is opened once and kept nowhere.
    if let Some(url) = c.checkout_start(&plan, &action, profile, &language, channel) {
        launch(&url);
    }
}
#[tauri::command(async)]
fn checkout_retry(c: C) {
    if let Some(url) = c.checkout_retry() {
        launch(&url);
    }
}
#[tauri::command]
fn checkout_check(c: C) {
    c.checkout_check();
}
#[tauri::command]
fn checkout_reset(c: C) {
    c.checkout_reset();
}
#[tauri::command(async)]
fn email_link_start(c: C, email: String, locale: String) -> Value {
    c.email_link_start(&email, &locale)
}
#[tauri::command(async)]
fn email_link_confirm(c: C, email: String, code: String) -> Value {
    c.email_link_confirm(&email, &code)
}
#[tauri::command(async)]
fn email_unlink(c: C, id: String) -> Value {
    c.email_unlink(&id)
}
#[tauri::command]
fn open_external(url: String) {
    if marz_core::links::allowed_external(&url) {
        launch(&url);
    }
}

fn main() {
    // Started at login. If the switch was left ON the computer is blocked and only the app can bring the
    // tunnel back, so it starts in the tray; otherwise there is nothing to do, and it leaves no trace.
    let at_login = std::env::args().any(|a| a == "--resume-if-armed");
    if at_login && !Controller::helper_blocking(&HelperClient::new(helper_endpoint())) {
        return;
    }
    let dir = data_dir();
    let vault = Arc::new(Vault::open(&dir, key_source().as_ref()));
    let api = AppApi::new(api_endpoint(), vault);
    let controller = Controller::new(api, HelperClient::new(helper_endpoint()), &dir, env!("MARZ_RELEASE_VERSION"), cfg!(feature = "lab"));
    tauri::Builder::default()
        // A second launch (menu, desktop) brings up the running app instead of starting another one.
        .plugin(tauri_plugin_single_instance::init(|app, _, _| tray::show_window(app)))
        .setup(move |app| {
            controller.start();
            app.manage(controller.clone());
            tray::install(app.handle(), controller.clone())?;
            if at_login {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            }
            Ok(())
        })
        // The cross hides the window; the app keeps running in the tray. Quit is in the tray menu.
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
                if let Some(c) = window.app_handle().try_state::<Arc<Controller>>() {
                    c.on_visible(false);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_state, toggle, select_server, route_check_start, route_check_stop, telegram_start, telegram_resume, telegram_cancel,
            email_send, email_verify, email_resend, email_reset, logout, mark_welcome_seen, set_theme, set_language, on_visible,
            checkout_start, checkout_retry, checkout_check, checkout_reset, email_link_start, email_link_confirm, email_unlink, open_external, emergency_off, set_kill_switch
        ])
        .run(tauri::generate_context!())
        .expect("window");
}
