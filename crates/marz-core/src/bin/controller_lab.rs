//! The app's whole behaviour without a window: controller + real App API code (lab) + real helper, with a
//! stub in place of the VPN engine. Prints PASS/FAIL. See tests/controller-lab/run.sh.
use marz_core::api::AppApi;
use marz_core::controller::{Controller, TelegramUi, View};
use marz_core::helper_client::HelperClient;
use marz_core::http::Endpoint;
use marz_core::vault::{self, FixedKey, Vault};
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

static FAILS: AtomicU32 = AtomicU32::new(0);

fn check(name: &str, ok: bool, detail: impl std::fmt::Display) {
    println!("{} {name} — {detail}", if ok { "PASS" } else { "FAIL" });
    if !ok {
        FAILS.fetch_add(1, Ordering::SeqCst);
    }
}

fn wait(c: &Controller, secs: u64, f: impl Fn(&View) -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(secs) {
        if f(&c.view()) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (url, dir, socket, bot, helper_cmd) = (&a[1], Path::new(&a[2]), &a[3], &a[4], &a[5]);
    let config = marz_core::tls::client_config_with(rustls::RootCertStore::empty()).unwrap();
    let vault = Arc::new(Vault::open(dir, &FixedKey([9; 32])));
    let api = AppApi::new(Endpoint::parse_with(url, config).unwrap(), vault);
    let helper = || HelperClient::new(socket.clone());
    let c = Controller::new(api, helper(), dir, "test", true);
    c.start();

    check("start.signed_out", !c.view().signed_in, "fresh install");
    check("start.no_config", !c.view().config_ready, "button locked");
    check("helper.seen", wait(&c, 5, |v| v.helper_ok && v.engine == "disconnected"), "helper answers, tunnel off");

    // Sign-in, as a user does it.
    c.telegram_start();
    let code = wait(&c, 5, |v| matches!(v.telegram, TelegramUi::Waiting { .. }));
    check("login.number_shown", code, "number first, Telegram only on the button");
    let done = Command::new("sh").args(["-c", bot]).status().map(|s| s.success()).unwrap_or(false);
    check("login.bot_confirmed", done, "lab bot picked the same number");
    check("login.signed_in", wait(&c, 10, |v| v.signed_in && v.config_ready), "sync finished after confirmation");
    let v = c.view();
    check("sync.account", v.account.is_some() && v.subscription.as_ref().is_some_and(|s| s.can_connect), "active plan");
    check("sync.routes", v.servers.len() >= 2 && v.from_account, format!("{} routes from the account", v.servers.len()));

    // Kill Switch setting: on by default, off only by choice, and it lasts.
    check("setting.on_by_default", c.view().prefs.kill_switch, "fresh install");
    let older: marz_core::prefs::Prefs = serde_json::from_str(r#"{"theme":"dark"}"#).unwrap();
    check("setting.older_prefs_read_as_on", older.kill_switch, "file from before the setting");
    check("setting.changes_while_off", c.set_kill_switch(false) && !c.view().prefs.kill_switch, "VPN off: allowed");
    let saved = std::fs::read_to_string(dir.join("prefs.json")).unwrap_or_default();
    let reread: marz_core::prefs::Prefs = serde_json::from_str(&saved).unwrap_or_default();
    check("setting.saved", !reread.kill_switch, "survives a restart");
    c.toggle();
    check("ks_off.connects", wait(&c, 15, |v| v.engine == "connected" && v.wanted), "tunnel without the block");
    check("ks_off.no_block", c.view().kill_switch == "off", "user's setting respected");
    check("setting.locked_while_on", !c.set_kill_switch(true) && !c.view().prefs.kill_switch, "cannot change with the VPN on");
    c.toggle();
    check("ks_off.off", wait(&c, 10, |v| v.engine == "disconnected" && !v.wanted), "big button");
    check("setting.back_on", c.set_kill_switch(true) && c.view().prefs.kill_switch, "VPN off again: allowed");

    // Connect.
    c.toggle();
    check("vpn.connects", wait(&c, 15, |v| v.engine == "connected" && v.wanted), "button -> helper -> engine running");
    check("vpn.active_from_engine", wait(&c, 8, |v| v.active_route.is_some()), format!("{:?}", c.view().active_route.is_some()));

    // Route switch.
    let id = c.view().servers[1].id.clone();
    c.select_server(Some(id.clone()));
    let tag = format!("srv-{id}");
    check("route.switched", wait(&c, 8, |v| v.active_route.as_deref() == Some(&tag)), "engine reports the chosen route");
    check("route.saved", c.view().prefs.server.as_deref() == Some(id.as_str()), "choice survives restart");
    let cached = c.view();
    check("route.no_restart", cached.engine == "connected" && cached.restarts == 0, format!("engine={} restarts={} error={:?}", cached.engine, cached.restarts, cached.engine_error));

    // No way out: the switch stays on, and the state says what is really happening.
    c.check_tunnel();
    c.check_tunnel();
    let v = c.view();
    check("loss.state_is_honest", v.traffic == "no_network" || v.traffic == "lost", format!("traffic={}", v.traffic));
    check("loss.switch_stays_on", v.wanted && v.engine == "connected", "not turned off by a failed check");

    // Engine crash: the switch stays on; it comes back by itself.
    let _ = Command::new("pkill").args(["-9", "-f", "run -c stdin"]).status();
    check("crash.reconnecting_shown", wait(&c, 5, |v| v.engine == "connecting" && v.wanted), "state is connecting, switch ON");
    check("crash.back", wait(&c, 20, |v| v.engine == "connected" && v.wanted), "engine restarted with the same route");
    check("crash.route_kept", wait(&c, 8, |v| v.active_route.as_deref() == Some(&tag)), "same route after the restart");

    // The block is on as long as the switch, whatever the tunnel does.
    check("kill_switch.armed", c.view().kill_switch == "armed", "firewall table up while connected");

    // Helper killed: it comes back blocking and without credentials. The app holds them and resumes by itself.
    let _ = Command::new("pkill").args(["-9", "-x", "marzvpn-helper"]).status();
    wait(&c, 5, |v| !v.helper_ok);
    let _ = Command::new("sh").args(["-c", helper_cmd.as_str()]).status();
    check("helper_crash.app_resumes", wait(&c, 20, |v| v.helper_ok && v.engine == "connected" && v.wanted), "needs_request -> Start, no user action");
    check("helper_crash.still_armed", c.view().kill_switch == "armed", "saved state brought the block back");
    check("helper_crash.choice_kept", wait(&c, 8, |v| v.active_route.as_deref() == Some(&tag)), "same route");

    // Off.
    c.toggle();
    check("vpn.off", wait(&c, 10, |v| v.engine == "disconnected" && !v.wanted), "user's button turns it off");
    check("kill_switch.off", c.view().kill_switch == "off", "firewall table gone with the switch");

    // A new process with the same data: session and choice are there, no sign-in.
    let data = std::fs::read(dir.join("vault.bin")).unwrap_or_default();
    check("vault.no_plain_tokens", !data.windows(8).any(|w| w == b"vless://") && !String::from_utf8_lossy(&data).contains("access"), "file is ciphertext");

    // Sign-out with the tunnel on ends the tunnel and removes this account's config.
    c.toggle();
    wait(&c, 15, |v| v.engine == "connected");
    c.logout();
    check("logout.signed_out", wait(&c, 10, |v| !v.signed_in), "local first");
    check("logout.tunnel_stopped", wait(&c, 10, |v| v.engine == "disconnected" && !v.wanted), "session end stops the tunnel");
    check("logout.config_removed", !c.view().config_ready, "button locked again");
    let reopened = Vault::open(dir, &FixedKey([9; 32]));
    check("logout.vault_empty", reopened.get(vault::TOKENS).is_none() && reopened.get(vault::ENGINE_CONFIG).is_none(), "tokens and routes gone");
    check("logout.device_kept", reopened.get(vault::DEVICE).is_some(), "device identity stays");

    let f = FAILS.load(Ordering::SeqCst);
    println!("=== {f} failure(s)");
    std::process::exit(i32::from(f > 0));
}
