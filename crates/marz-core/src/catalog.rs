//! The plans the app offers come from the server's public catalog (GET /v1/catalog), never from the app itself.
//! Only entries that pass the checks below are shown: Basic, and the Pro codes only when the entry carries the Pro
//! descriptor. With Pro off on the server the catalog lists the previous plan without that descriptor, so the app
//! offers Basic alone and never the previous plan. No catalog at all: the last valid one, else built-in Basic only.
//! Numbers are the server's; the app formats them and enforces nothing.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Sent with a checkout of a Pro code: this app showed the Pro descriptor (server app_api/plans.py PRO_CATALOG_ACK).
pub const PRO_ACK: &str = "pro-1";
/// Wire codes stay those of the previous plan; the server sells them as Pro.
pub const PRO_CODES: [&str; 2] = ["unlimited", "unlimited_6m"];

pub fn is_pro_code(code: &str) -> bool {
    PRO_CODES.contains(&code)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProOffer {
    /// Bytes at full speed in each package, decimal (300 GB = 300 * 10^9).
    pub full_speed_bytes: u64,
    pub package_days: u32,
    pub packages: u32,
    /// Speed after the full-speed bytes of a package are used, bits per second.
    pub after_full_speed_bps: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub code: String,
    pub period_days: u32,
    /// Basic: binary bytes (100 GiB). Pro: none, there is no data cap.
    pub quota_bytes: Option<u64>,
    /// As the server writes it, e.g. "4.00".
    pub price_usd: String,
    pub pro: Option<ProOffer>,
}

fn price_ok(p: &str) -> bool {
    let (int, frac) = p.split_once('.').unwrap_or((p, "00"));
    (1..=4).contains(&int.len())
        && int.bytes().all(|b| b.is_ascii_digit())
        && frac.len() == 2
        && frac.bytes().all(|b| b.is_ascii_digit())
        && p.bytes().any(|b| (b'1'..=b'9').contains(&b))
}

fn uint(v: &Value, key: &str) -> Option<u64> {
    v.get(key).and_then(Value::as_u64)
}

fn pro_offer(entry: &Value, period_days: u32) -> Option<ProOffer> {
    if entry.get("product").and_then(Value::as_str) != Some("pro") {
        return None;
    }
    if !entry.get("quota_bytes").is_none_or(Value::is_null) {
        return None;
    }
    let p = entry.get("pro").filter(|p| p.is_object())?;
    let offer = ProOffer {
        full_speed_bytes: uint(p, "full_speed_bytes")?,
        package_days: u32::try_from(uint(p, "package_days")?).ok()?,
        packages: u32::try_from(uint(p, "packages")?).ok()?,
        after_full_speed_bps: uint(p, "after_full_speed_bps")?,
    };
    let valid = offer.full_speed_bytes > 0
        && offer.after_full_speed_bps > 0
        && offer.package_days == 30
        && offer.packages >= 1
        && offer.packages.checked_mul(30) == Some(period_days);
    valid.then_some(offer)
}

fn plan(entry: &Value) -> Option<Plan> {
    let code = entry.get("code").and_then(Value::as_str)?;
    let period_days = u32::try_from(uint(entry, "period_days")?).ok().filter(|d| (1..=3660).contains(d))?;
    let price_usd = entry.get("price_usd").and_then(Value::as_str).filter(|p| price_ok(p))?.to_string();
    match code {
        "basic" => {
            let quota = uint(entry, "quota_bytes").filter(|q| *q > 0)?;
            Some(Plan { code: code.into(), period_days, quota_bytes: Some(quota), price_usd, pro: None })
        }
        c if is_pro_code(c) => {
            let pro = pro_offer(entry, period_days)?;
            Some(Plan { code: code.into(), period_days, quota_bytes: None, price_usd, pro: Some(pro) })
        }
        _ => None,
    }
}

/// The valid plans of a catalog answer, in the server's order, each code once. Empty: nothing usable.
pub fn parse(catalog: &Value) -> Vec<Plan> {
    let mut out: Vec<Plan> = Vec::new();
    for p in catalog.get("plans").and_then(Value::as_array).into_iter().flatten().filter_map(plan) {
        if !out.iter().any(|o| o.code == p.code) {
            out.push(p);
        }
    }
    out
}

/// No catalog was ever read: Basic only, $2 for 30 days with 100 GiB. Never a Pro offer.
pub fn builtin() -> Vec<Plan> {
    parse(&json!({"plans": [{"code": "basic", "period_days": 30, "quota_bytes": 100u64 * 1024 * 1024 * 1024, "price_usd": "2.00"}]}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn production_pro_off() -> Value {
        json!({"plans": [{"code": "basic", "period_days": 30, "quota_bytes": 107374182400u64, "price_usd": "2.00"}, {"code": "unlimited", "period_days": 30, "quota_bytes": null, "price_usd": "4.00"}, {"code": "unlimited_6m", "period_days": 180, "quota_bytes": null, "price_usd": "20.00"}], "trial": {"days": 3, "quota_bytes": 3221225472u64, "channel": "telegram_bot", "bot": "MarzReality_bot"}, "terms_url": "https://marzvpn.com/terms"})
    }

    fn pro_block(packages: u64) -> Value {
        json!({"full_speed_bytes": 300000000000u64, "package_days": 30, "packages": packages, "after_full_speed_bps": 2000000, "speed_limit_scope": "account", "data_cap": null})
    }

    fn production_pro_on() -> Value {
        let mut c = production_pro_off();
        for (i, n) in [(1, 1), (2, 6)] {
            let e = &mut c["plans"][i];
            e["product"] = json!("pro");
            e["name"] = json!("Pro");
            e["pro"] = pro_block(n);
        }
        c
    }

    fn codes(p: &[Plan]) -> Vec<&str> {
        p.iter().map(|x| x.code.as_str()).collect()
    }

    #[test]
    fn pro_off_offers_basic_only() {
        let p = parse(&production_pro_off());
        assert_eq!(codes(&p), ["basic"]);
        assert_eq!(p[0], Plan { code: "basic".into(), period_days: 30, quota_bytes: Some(100 * 1024 * 1024 * 1024), price_usd: "2.00".into(), pro: None });
    }

    #[test]
    fn pro_on_offers_basic_and_both_pro_plans() {
        let p = parse(&production_pro_on());
        assert_eq!(codes(&p), ["basic", "unlimited", "unlimited_6m"]);
        assert_eq!(p[1].pro, Some(ProOffer { full_speed_bytes: 300_000_000_000, package_days: 30, packages: 1, after_full_speed_bps: 2_000_000 }));
        assert_eq!((p[2].period_days, p[2].price_usd.as_str(), p[2].pro.as_ref().unwrap().packages), (180, "20.00", 6));
        assert_eq!(p[1].quota_bytes, None);
    }

    #[test]
    fn a_damaged_pro_descriptor_hides_that_plan() {
        let bad: [(&str, Value); 10] = [
            ("/plans/1/product", json!("unlimited")),
            ("/plans/1/quota_bytes", json!(1000)),
            ("/plans/1/pro/full_speed_bytes", json!(0)),
            ("/plans/1/pro/after_full_speed_bps", json!(0)),
            ("/plans/1/pro/package_days", json!(31)),
            ("/plans/1/pro/packages", json!(2)),
            ("/plans/1/pro/full_speed_bytes", json!("300")),
            ("/plans/1/pro", json!(null)),
            ("/plans/1/price_usd", json!("free")),
            ("/plans/1/period_days", json!(31)),
        ];
        for (path, value) in bad {
            let mut c = production_pro_on();
            *c.pointer_mut(path).unwrap() = value;
            assert_eq!(codes(&parse(&c)), ["basic", "unlimited_6m"], "{path}");
        }
        let mut c = production_pro_on();
        c["plans"][2]["pro"]["packages"] = json!(1);
        assert_eq!(codes(&parse(&c)), ["basic", "unlimited"], "6 packages must make 180 days");
    }

    #[test]
    fn unknown_codes_duplicates_and_bad_basic_are_dropped() {
        let mut c = production_pro_on();
        c["plans"].as_array_mut().unwrap().push(json!({"code": "family", "period_days": 30, "quota_bytes": null, "price_usd": "9.00", "product": "pro", "pro": pro_block(1)}));
        c["plans"].as_array_mut().unwrap().push(json!({"code": "basic", "period_days": 30, "quota_bytes": 1, "price_usd": "0.10"}));
        assert_eq!(codes(&parse(&c)), ["basic", "unlimited", "unlimited_6m"]);
        assert_eq!(parse(&c)[0].quota_bytes, Some(107374182400));
        let mut c = production_pro_off();
        c["plans"][0]["quota_bytes"] = json!(null);
        assert!(parse(&c).is_empty(), "a Basic without its quota is not offered");
        assert!(parse(&json!({"error": "x"})).is_empty());
        assert!(parse(&json!("plans")).is_empty());
        for p in ["0.00", "2", "2.0", "-2.00", "20000.00", "2,00", ""] {
            let mut c = production_pro_off();
            c["plans"][0]["price_usd"] = json!(p);
            assert_eq!(parse(&c).is_empty(), p != "2", "{p}");
        }
    }

    #[test]
    fn builtin_is_basic_only() {
        let b = builtin();
        assert_eq!(codes(&b), ["basic"]);
        assert_eq!((b[0].period_days, b[0].price_usd.as_str(), b[0].quota_bytes), (30, "2.00", Some(107374182400)));
    }

    #[test]
    fn pro_codes() {
        assert!(is_pro_code("unlimited") && is_pro_code("unlimited_6m"));
        assert!(!is_pro_code("basic") && !is_pro_code("pro"));
    }
}
