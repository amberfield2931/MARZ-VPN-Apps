//! Contract check against the real App API code in its LAB mode (synthetic users, no production).
//! Prints PASS/FAIL lines; driven by tests/api-lab/run.sh, which plays the bot's part.
use marz_core::api::{AppApi, LoginProgress};
use marz_core::http::Endpoint;
use marz_core::vault::{FixedKey, Vault};
use marz_engine::EngineRequest;
use std::path::Path;
use std::sync::Arc;

fn check(name: &str, ok: bool, detail: impl std::fmt::Display) -> bool {
    println!("{} {name} — {detail}", if ok { "PASS" } else { "FAIL" });
    ok
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (url, dir, step) = (&args[1], &args[2], args[3].as_str());
    let config = marz_core::tls::client_config_with(rustls::RootCertStore::empty()).unwrap();
    let endpoint = Endpoint::parse_with(url, config).unwrap();
    let vault = Arc::new(Vault::open(Path::new(dir), &FixedKey([9; 32])));
    let api = AppApi::new(endpoint, vault);
    let mut ok = true;
    match step {
        "start-telegram" => {
            let l = api.start_telegram_login().unwrap();
            ok &= check("telegram.start", l.telegram_url.contains("start=applogin_") && l.match_code.len() == 2, "url and match code returned");
            ok &= check("telegram.pending_survives_restart", api.pending_telegram_login().as_ref() == Some(&l), "same request kept");
            ok &= check("telegram.poll_pending", api.poll_telegram_login().unwrap() == LoginProgress::Pending, "202 before the bot confirms");
        }
        "finish-telegram" => {
            ok &= check("telegram.confirmed", api.poll_telegram_login().unwrap() == LoginProgress::Done, "tokens saved, signed proof accepted");
            ok &= check("telegram.signed_in", api.is_signed_in(), "session present");
        }
        "reject-telegram" => {
            let e = api.poll_telegram_login().unwrap_err();
            ok &= check("telegram.rejected", e.code == "login_rejected" && !api.is_signed_in(), &e.code);
        }
        "data" => {
            let a = api.account().unwrap();
            ok &= check("account.parse", !a.identities.is_empty() && a.identities[0].kind == "telegram", format!("{} identity", a.identities.len()));
            let s = api.subscription().unwrap();
            ok &= check("subscription.parse", s.can_connect && s.entitlement.is_some(), format!("state={} kind={}", s.state, s.entitlement.as_ref().map_or("-", |e| e.kind.as_str())));
            ok &= check("subscription.offline_snapshot", api.offline_snapshot().is_some(), "cached encrypted");
            let list = api.servers().unwrap();
            ok &= check("servers.parse", !list.is_empty() && list.iter().all(|r| !r.id.is_empty()), format!("{} routes", list.len()));
            ok &= check("servers.no_endpoint_exposed", !format!("{list:?}").contains("127.0.0.1"), "entry addresses absent");
            let config = api.vpn_config(None).unwrap();
            let req = EngineRequest::from_api_config(&config);
            let parsed = req.as_ref().map(|r| r.routes.len()).unwrap_or(0);
            ok &= check("config.real_api_config_accepted", req.is_ok(), format!("{parsed} typed routes, selected {}", req.as_ref().map_or("-", |r| r.selected.as_str())));
            let same = req.as_ref().map(|r| r.routes.len() == list.len()).unwrap_or(false);
            ok &= check("config.routes_match_list", same, "one outbound per listed route");
            let chosen = api.vpn_config(Some(&list[0].id)).unwrap();
            let sel = EngineRequest::from_api_config(&chosen).map(|r| r.selected).unwrap_or_default();
            ok &= check("config.selected_route", sel == format!("srv-{}", list[0].id), "server choice applied by the API");
        }
        "refresh" => {
            // Lab runs with a 1 s access TTL: the first call must rotate silently and succeed.
            std::thread::sleep(std::time::Duration::from_millis(2100));
            let a = api.account();
            ok &= check("refresh.rotates_and_retries_once", a.is_ok(), format!("{:?}", a.as_ref().map(|_| "ok").map_err(|e| e.code.clone())));
            std::thread::sleep(std::time::Duration::from_millis(2100));
            ok &= check("refresh.second_rotation", api.subscription().is_ok(), "new refresh token accepted");
        }
        "email-start" => {
            ok &= check("email.start", api.start_email_login(&args[4], "ru").is_ok(), "202");
        }
        "email-verify" => {
            let wrong = api.verify_email_code(&args[4], "000000");
            ok &= check("email.wrong_code", wrong.as_ref().err().is_some_and(|e| e.code == "invalid_code"), "refused");
            ok &= check("email.verify", api.verify_email_code(&args[4], &args[5]).is_ok() && api.is_signed_in(), "tokens saved");
            ok &= check("email.account", api.account().is_ok(), "signed-in session works");
        }
        "billing" => {
            let s = api.subscription().unwrap();
            let e = api.create_checkout_intent("basic", "renew", s.effective_id.as_deref(), "en", None);
            // Billing is off in the lab, so this covers the "feature off" answer only; a successful intent is NOT TESTED.
            ok &= check("billing.feature_off_answer", e.as_ref().err().is_some_and(|x| x.status == 404 && x.code == "not_found"), "lab answers 404 not_found, mapped to the bot/site hint; no link opened");
        }
        "logout" => {
            api.logout();
            ok &= check("logout.local", !api.is_signed_in(), "session removed");
            ok &= check("logout.account_data_gone", api.offline_snapshot().is_none(), "cache removed");
            ok &= check("logout.after", api.account().unwrap_err().code == "signed_out", "calls refused without a request");
        }
        _ => panic!("unknown step"),
    }
    std::process::exit(i32::from(!ok));
}
