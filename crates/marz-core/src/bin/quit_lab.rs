//! The tray's Quit without a window, on the target's real pipe or socket: the Controller's own disconnect_for_quit against the real helper.
//! Windows has no display in the Wine lab, so this is how its Quit runs there. Prints one line per step; see tests/windows/wine-quit-test.sh.
//! Usage: quit_lab <data dir> <endpoint> status|quit|ask_cancel|emergency
use marz_core::api::AppApi;
use marz_core::controller::{Controller, QuitStart, QuitUi};
use marz_core::helper_client::HelperClient;
use marz_core::http::Endpoint;
use marz_core::vault::{FixedKey, Vault};
use std::path::Path;
use std::sync::Arc;

fn controller(dir: &Path, helper: HelperClient) -> Arc<Controller> {
    std::fs::create_dir_all(dir).unwrap();
    let config = marz_core::tls::client_config().unwrap();
    let api = AppApi::new(Endpoint::parse_with("http://127.0.0.1:9", config).unwrap(), Arc::new(Vault::open(dir, &FixedKey([9; 32]))));
    Controller::new(api, helper, dir, "lab", true)
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (dir, endpoint, step) = (Path::new(&a[1]), a[2].as_str(), a[3].as_str());
    let helper = HelperClient::new(endpoint);
    match step {
        "status" => println!("{}", helper.status().map_or_else(|e| format!("error {e:?}"), |s| format!("state={:?} wanted={} armed={} session={:?} error={:?}", s.state, s.wanted, s.kill_switch == marz_engine::ipc::KillSwitch::Armed, s.session, s.error))),
        "ask_cancel" => {
            let c = controller(dir, helper);
            let start = c.begin_quit();
            let asked = c.view().quit == QuitUi::Confirm;
            c.quit_cancel();
            println!("{start:?} asked={asked} after={:?}", c.view().quit);
        }
        "emergency" => println!("{:?}", controller(dir, helper).emergency_off()),
        "quit" => {
            let c = controller(dir, helper);
            // As the tray and a yes in the question do it. Err is what the window would show.
            let result = match c.begin_quit() {
                QuitStart::Close => Some(Ok(())),
                _ if matches!(c.view().quit, QuitUi::Failed { .. }) => Some(Err(match c.view().quit {
                    QuitUi::Failed { code } => code,
                    _ => String::new(),
                })),
                _ => c.disconnect_for_quit(),
            };
            println!("{result:?}");
        }
        _ => std::process::exit(2),
    }
}
