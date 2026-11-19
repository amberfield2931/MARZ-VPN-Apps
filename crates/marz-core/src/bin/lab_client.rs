//! Contract check against the real App API code in its LAB mode (synthetic users, no production).
//! Prints PASS/FAIL lines; driven by tests/api-lab/run.sh, which plays the bot's part.
use marz_core::api::{AppApi, CodeEntry, LoginProgress, TelegramFlow};
use marz_core::http::Endpoint;
use marz_core::vault::{FixedKey, Vault};
use marz_engine::EngineRequest;
use std::path::Path;
use std::sync::Arc;

fn check(name: &str, ok: bool, detail: impl std::fmt::Display) -> bool {
    println!("{} {name} — {detail}", if ok { "PASS" } else { "FAIL" });
    ok
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64
}

/// A code that is surely wrong: the right one from the file, last digit changed.
fn wrong(path: &str) -> String {
    let code = std::fs::read_to_string(path).unwrap();
    let mut c: Vec<u8> = code.trim().bytes().collect();
    c[5] = b'0' + (c[5] - b'0' + 1) % 10;
    String::from_utf8(c).unwrap()
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
            // A server without the code flow (LAB_TELEGRAM_CODE_LOGIN=0): the app asks for it and gets number matching.
            let l = api.start_telegram_login("ru").unwrap();
            ok &= check("telegram.classic_fallback", l.flow == TelegramFlow::Classic && l.verification_url.is_empty(), "no verification_url: number matching");
            ok &= check("telegram.start", l.telegram_url.contains("start=applogin_") && l.match_code.len() == 2, "url and match code returned");
            ok &= check("telegram.pending_survives_restart", api.pending_telegram_login().as_ref() == Some(&l), "same request kept");
            ok &= check("telegram.poll_pending", matches!(api.poll_telegram_login().unwrap(), LoginProgress::Pending(_)), "202 before the bot confirms");
        }
        "finish-telegram" | "finish" => {
            ok &= check("telegram.confirmed", api.poll_telegram_login().unwrap() == LoginProgress::Done, "tokens saved, signed proof accepted");
            ok &= check("telegram.signed_in", api.is_signed_in(), "session present");
            ok &= check("telegram.pending_cleared", api.pending_telegram_login().is_none(), "request removed");
        }
        "reject-telegram" => {
            let e = api.poll_telegram_login().unwrap_err();
            ok &= check("telegram.rejected", e.code == "login_rejected" && !api.is_signed_in(), &e.code);
        }
        "start-telegram-code" => {
            let started = now();
            let l = api.start_telegram_login(&args[4]).unwrap();
            let prefix = match args[4].as_str() { "ru" => "/ru", "fa" => "/fa", "zh-cn" => "/zh-cn", _ => "" };
            ok &= check("code.flow", l.flow == TelegramFlow::Code && l.match_code.is_empty(), "verification_url answered: code flow");
            let r = l.reference.as_bytes();
            ok &= check("code.ref", r.len() == 9 && r[4] == b'-' && l.reference.replace('-', "").bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()), "request number XXXX-XXXX");
            ok &= check("code.page_link", marz_core::links::is_device_page(&l.verification_url) && l.verification_url.starts_with(&format!("https://marzvpn.com{prefix}/device/#c=")), format!("page in the asked language ({})", args[4]));
            // The lab's bot is MarzLabBot, which the app's allow-list never opens; production's is MarzReality_bot.
            let bot_ok = l.telegram_url.starts_with("https://t.me/") && l.telegram_url.ends_with(&format!("?start=login_{}", l.reference.replace('-', "")));
            ok &= check("code.bot_link", bot_ok && !marz_core::links::allowed_external(&l.telegram_url), "t.me link of this request; the lab bot is not on the allow-list");
            ok &= check("code.expiry", (l.expires_at - started - 600).abs() <= 5, format!("{} s", l.expires_at - started));
            ok &= check("code.pending_survives_restart", api.pending_telegram_login().as_ref() == Some(&l), "same request kept");
            match api.poll_telegram_login().unwrap() {
                LoginProgress::Pending(p) => ok &= check("code.poll_progress", p.progress.as_deref() == Some("waiting") && p.attempts_left == Some(5), format!("{p:?}")),
                other => ok &= check("code.poll_progress", false, format!("{other:?}")),
            }
        }
        "poll-progress" => match api.poll_telegram_login() {
            Ok(LoginProgress::Pending(p)) => ok &= check(&format!("code.progress_{}", args[4]), p.progress.as_deref() == Some(args[4].as_str()), format!("{p:?}")),
            other => ok &= check(&format!("code.progress_{}", args[4]), false, format!("{other:?}")),
        },
        "enter-code-early" => {
            let r = api.enter_telegram_code("123456");
            ok &= check("code.not_requested", matches!(&r, Ok(CodeEntry::Refused { code, .. }) if code == "code_not_requested"), format!("{r:?}"));
            let f = api.enter_telegram_code("12a45");
            ok &= check("code.bad_format_local", matches!(&f, Ok(CodeEntry::Refused { code, .. }) if code == "invalid_code_format"), format!("{f:?}"));
            ok &= check("code.request_kept", api.pending_telegram_login().is_some(), "still waiting");
        }
        "enter-wrong-code" => {
            let r = api.enter_telegram_code(&wrong(&args[4]));
            ok &= check("code.wrong", matches!(r, Ok(CodeEntry::Refused { ref code, attempts_left: Some(4) }) if code == "invalid_code"), format!("{r:?}"));
            ok &= check("code.wrong_keeps_request", api.pending_telegram_login().is_some() && !api.is_signed_in(), "still waiting");
        }
        "enter-code-file" => {
            let code = std::fs::read_to_string(&args[4]).unwrap();
            // Typed with a space in the middle, as people do: the app sends six digits.
            let typed = format!("{} {}", &code.trim()[..3], &code.trim()[3..]);
            let r = api.enter_telegram_code(&typed);
            ok &= check("code.in_app_signed_in", r == Ok(CodeEntry::SignedIn) && api.is_signed_in(), format!("{r:?}"));
            ok &= check("code.pending_cleared", api.pending_telegram_login().is_none(), "request removed");
        }
        "enter-code-after-page" => {
            let code = std::fs::read_to_string(&args[4]).unwrap();
            let r = api.enter_telegram_code(code.trim());
            ok &= check("code.already_approved_keeps_polling", r == Ok(CodeEntry::Approved) && api.pending_telegram_login().is_some(), format!("{r:?}"));
        }
        "reject-telegram-code" => {
            let e = api.poll_telegram_login().unwrap_err();
            ok &= check("code.rejected", e.code == "login_rejected" && !api.is_signed_in() && api.pending_telegram_login().is_none(), &e.code);
        }
        "burn" => {
            let bad = wrong(&args[4]);
            let mut seen = Vec::new();
            for _ in 0..4 {
                seen.push(api.enter_telegram_code(&bad));
            }
            let left: Vec<_> = seen.iter().map(|r| match r { Ok(CodeEntry::Refused { attempts_left, .. }) => *attempts_left, _ => None }).collect();
            ok &= check("code.attempts_count_down", left == [Some(4), Some(3), Some(2), Some(1)], format!("{left:?}"));
            let last = api.enter_telegram_code(&bad);
            ok &= check("code.fifth_wrong_burns", matches!(&last, Err(e) if e.code == "login_code_attempts"), format!("{last:?}"));
            ok &= check("code.burned_request_removed", api.pending_telegram_login().is_none() && !api.is_signed_in(), "start again");
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
