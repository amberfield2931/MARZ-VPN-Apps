//! Payment for the signed-in client's own profile: rules from Android RC6 api/Checkout.kt. The link is
//! one-use, never stored or logged, and only the two exact shapes the server builds are ever opened.
pub const SITE_HOST: &str = "marzvpn.com";
pub const BOT_HOST: &str = "t.me";
pub const BOT_USERNAME: &str = "MarzReality_bot";
pub const STATES: [&str; 8] = ["awaiting_checkout", "pending", "fulfilling", "paid", "failed", "expired", "cancelled", "unavailable"];

pub fn api_locale(language: &str) -> &'static str {
    match language {
        "ru" => "ru",
        "fa" => "fa",
        "zh" | "zh-CN" | "zh-cn" => "zh-cn",
        _ => "en",
    }
}

fn is_handoff(s: &str) -> bool {
    s.len() == 32 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

pub fn is_operation(id: &str) -> bool {
    id.strip_prefix("bill_").is_some_and(|r| r.len() == 22 && r.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'))
}

/// Exactly: the website checkout of the requested plan in the requested language, or the bot's
/// `apppay_` start link. Another host, path, plan, port, user info or parameter is refused.
pub fn validate_link(channel: &str, url: &str, plan: &str, locale: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else { return false };
    let (rest, fragment) = match rest.split_once('#') {
        Some((r, f)) => (r, Some(f)),
        None => (rest, None),
    };
    let (rest, query) = match rest.split_once('?') {
        Some((r, q)) => (r, Some(q)),
        None => (rest, None),
    };
    let (host, path) = match rest.split_once('/') {
        Some((h, p)) => (h, format!("/{p}")),
        None => (rest, String::new()),
    };
    match channel {
        "website" => {
            let prefix = if locale == "en" { String::new() } else { format!("/{locale}") };
            host == SITE_HOST
                && path == format!("{prefix}/checkout")
                && query == Some(format!("plan={plan}&app=1").as_str())
                && fragment.and_then(|f| f.strip_prefix("app=")).is_some_and(is_handoff)
        }
        "telegram_bot" => {
            host == BOT_HOST
                && path == format!("/{BOT_USERNAME}")
                && fragment.is_none()
                && query.and_then(|q| q.strip_prefix("start=apppay_")).is_some_and(is_handoff)
        }
        _ => false,
    }
}

/// What the user is told when no payment link was opened (RC6 CheckoutProblem).
pub fn problem(code: &str, status: u16) -> &'static str {
    match code {
        "not_found" => "feature_off",
        "signed_out" | "unauthorized" | "reauth_required" => "signed_out",
        "pilot_closed" => "pilot_closed",
        "vpn_identity_not_found" => "profile_gone",
        "select_existing_profile" => "choose_profile",
        "existing_profile_required" => "no_profile",
        "verified_email_required" | "website_referral_requires_bot" | "billing_ownership_requires_review" | "vpn_ownership_conflict" | "unsupported_billing_profile" => "review",
        "website_billing_unavailable" | "upstream_unavailable" => "unavailable",
        "device_signature_required" | "invalid_device_signature" => "clock",
        "network" => "network",
        "handoff_expired_or_used" | "handoff_expired" => "expired",
        _ if status == 404 => "feature_off",
        _ if status == 429 => "too_many",
        _ if status >= 500 => "unavailable",
        _ => "generic",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const H: &str = "abcdefghijklmnopqrstuvwxyz012345";

    #[test]
    fn exact_website_link() {
        assert!(validate_link("website", &format!("https://marzvpn.com/checkout?plan=basic&app=1#app={H}"), "basic", "en"));
        assert!(validate_link("website", &format!("https://marzvpn.com/ru/checkout?plan=unlimited&app=1#app={H}"), "unlimited", "ru"));
    }

    #[test]
    fn exact_bot_link() {
        assert!(validate_link("telegram_bot", &format!("https://t.me/MarzReality_bot?start=apppay_{H}"), "basic", "en"));
    }

    #[test]
    fn everything_else_is_refused() {
        let bad = [
            ("website", format!("http://marzvpn.com/checkout?plan=basic&app=1#app={H}")),
            ("website", format!("https://evil.com/checkout?plan=basic&app=1#app={H}")),
            ("website", format!("https://marzvpn.com:444/checkout?plan=basic&app=1#app={H}")),
            ("website", format!("https://user@marzvpn.com/checkout?plan=basic&app=1#app={H}")),
            ("website", format!("https://marzvpn.com/ru/checkout?plan=basic&app=1#app={H}")),
            ("website", format!("https://marzvpn.com/checkout?plan=unlimited&app=1#app={H}")),
            ("website", format!("https://marzvpn.com/checkout?plan=basic&app=1&x=1#app={H}")),
            ("website", "https://marzvpn.com/checkout?plan=basic&app=1#app=short".to_string()),
            ("website", "https://marzvpn.com/checkout?plan=basic&app=1".to_string()),
            ("telegram_bot", format!("https://t.me/OtherBot?start=apppay_{H}")),
            ("telegram_bot", format!("https://t.me/MarzReality_bot?start=applogin_{H}")),
            ("telegram_bot", format!("https://t.me/MarzReality_bot?start=apppay_{H}#x")),
            ("other", format!("https://marzvpn.com/checkout?plan=basic&app=1#app={H}")),
        ];
        for (channel, url) in bad {
            assert!(!validate_link(channel, &url, "basic", "en"), "{url}");
        }
    }

    #[test]
    fn problems_map_like_android() {
        assert_eq!(problem("vpn_identity_not_found", 404), "profile_gone");
        assert_eq!(problem("anything", 404), "feature_off");
        assert_eq!(problem("anything", 429), "too_many");
        assert_eq!(problem("anything", 500), "unavailable");
        assert_eq!(problem("anything", 400), "generic");
        assert_eq!(problem("invalid_device_signature", 401), "clock");
    }

    #[test]
    fn operation_ids() {
        assert!(is_operation("bill_0123456789abcdefghijkl"));
        assert!(!is_operation("bill_short"));
        assert!(!is_operation("xbill_0123456789abcdefghijkl"));
    }
}
