//! Read models, parsed exactly as Android RC6 HttpAppApi does: optional fields stay optional and nothing
//! unconfirmed is invented. Serialized to the UI as is; names and texts are chosen by the UI.
use serde::{Deserialize, Serialize};
use serde_json::Value;

fn opt_str(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    pub kind: String,
    pub display: String,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub identities: Vec<Identity>,
    pub vpn_sources: Vec<(String, String)>,
}

impl Account {
    pub fn parse(v: &Value) -> Option<Self> {
        let identities = v.get("identities").and_then(Value::as_array).map_or(Vec::new(), |a| {
            a.iter()
                .filter_map(|o| {
                    Some(Identity {
                        kind: if o.get("kind")?.as_str()? == "telegram" { "telegram".into() } else { "email".into() },
                        display: o.get("display")?.as_str()?.to_string(),
                        id: opt_str(o, "id").unwrap_or_default(),
                    })
                })
                .collect()
        });
        let vpn_sources = v.get("vpn_identities").and_then(Value::as_array).map_or(Vec::new(), |a| {
            a.iter().filter_map(|o| Some((o.get("id")?.as_str()?.to_string(), opt_str(o, "source").unwrap_or_default()))).collect()
        });
        Some(Account { id: v.get("id")?.as_str()?.to_string(), identities, vpn_sources })
    }
}

/// The current 30-day package of a Pro plan, as the server reports it. Display only: the limits are enforced by the
/// server, never here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProStatus {
    /// Decimal bytes (300 GB = 300 * 10^9).
    pub limit_bytes: i64,
    pub remaining_full_speed_bytes: i64,
    pub limited: bool,
    /// Bits per second while limited.
    pub normal_speed_limit_bps: Option<i64>,
    /// When full speed comes back: the next package, or none when this is the plan's last one.
    pub next_package_at: Option<i64>,
    pub plan_end: i64,
    /// The server's numbers are older than it trusts: the app says it is checking instead of showing them.
    pub stale: bool,
}

impl ProStatus {
    /// None when anything does not add up: the app then says it is checking the plan, and shows no numbers.
    fn parse(p: &Value) -> Option<Self> {
        let num = |k: &str| p.get(k).and_then(Value::as_i64);
        let s = ProStatus {
            limit_bytes: num("limit_bytes").filter(|l| *l > 0)?,
            remaining_full_speed_bytes: num("remaining_full_speed_bytes").filter(|r| *r >= 0)?,
            limited: p.get("limited").and_then(Value::as_bool)?,
            normal_speed_limit_bps: num("normal_speed_limit_bps").filter(|b| *b > 0),
            next_package_at: num("next_package_at").filter(|t| *t > 0),
            plan_end: num("plan_end").filter(|t| *t > 0)?,
            stale: p.get("stale").and_then(Value::as_bool).unwrap_or(true),
        };
        let used = num("used_bytes").filter(|u| *u >= 0)?;
        let consistent = s.remaining_full_speed_bytes <= s.limit_bytes
            && s.remaining_full_speed_bytes == (s.limit_bytes - used).max(0)
            && (!s.limited || s.normal_speed_limit_bps.is_some())
            && s.next_package_at.is_none_or(|n| n <= s.plan_end);
        consistent.then_some(s)
    }
}

/// `kind`: trial | basic | pro | unlimited | verifying | other. `unlimited` is a plan paid before Pro, kept until its
/// paid end; its old name is never shown. `verifying`: a paid plan the server is still checking. Other = older or special
/// profiles: Marzban's own numbers, without inventing a plan name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entitlement {
    pub kind: String,
    pub ends_at: i64,
    pub quota_bytes: Option<i64>,
    pub used_bytes: Option<i64>,
    pub bonus_bytes: Option<i64>,
    /// kind pro only; None there means "checking plan details".
    #[serde(default)]
    pub pro: Option<ProStatus>,
}

impl Entitlement {
    pub fn parse(e: &Value) -> Self {
        let ends_at = e.get("ends_at").and_then(Value::as_i64).unwrap_or(0);
        let quota = e.get("quota_bytes").and_then(Value::as_i64);
        let used = e.get("used_bytes").and_then(Value::as_i64).unwrap_or(0);
        let bonus = e.pointer("/bonus/reserve_bytes").and_then(Value::as_i64).filter(|b| *b > 0);
        let kind = e.get("kind").and_then(Value::as_str).unwrap_or("");
        let plain = |kind: &str, quota_bytes, used_bytes| Entitlement { kind: kind.into(), ends_at, quota_bytes, used_bytes, bonus_bytes: bonus, pro: None };
        match kind {
            // Pro has no data cap: its package numbers live only under `pro`, never as a quota.
            "pro" => Entitlement { pro: e.get("pro").and_then(ProStatus::parse), ..plain("pro", None, None) },
            "subscription" => plain("verifying", None, None),
            "unlimited" => plain("unlimited", None, None),
            "trial" => plain("trial", Some(quota.unwrap_or(0)), Some(used)),
            "basic" if quota.is_some() => plain("basic", quota, Some(used)),
            "basic" => plain("other", None, None),
            _ => plain("other", quota, quota.map(|_| used)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VpnProfile {
    pub id: String,
    pub entitlement: Option<Entitlement>,
    pub state: String,
    pub queued: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subscription {
    pub state: String,
    pub entitlement: Option<Entitlement>,
    pub can_connect: bool,
    pub effective_id: Option<String>,
    pub profiles: Vec<VpnProfile>,
}

impl Subscription {
    pub fn parse(v: &Value) -> Self {
        let state = v.get("state").and_then(Value::as_str).unwrap_or("none").to_string();
        let effective = v.get("effective");
        let entitlement = effective.and_then(|e| e.get("entitlement")).filter(|e| e.is_object()).map(Entitlement::parse);
        let profiles = v.get("all").and_then(Value::as_array).map_or(Vec::new(), |a| {
            a.iter()
                .filter_map(|o| {
                    let id = opt_str(o, "vpn_identity")?;
                    let pe = o.get("entitlement").filter(|e| e.is_object());
                    Some(VpnProfile {
                        id,
                        entitlement: pe.map(Entitlement::parse),
                        state: pe.and_then(|e| e.get("state")).and_then(Value::as_str).unwrap_or("none").to_string(),
                        queued: pe.and_then(|e| e.get("queued")).and_then(Value::as_array).map_or(0, Vec::len),
                    })
                })
                .collect()
        });
        Subscription {
            can_connect: state == "active",
            state,
            entitlement,
            effective_id: effective.and_then(|e| opt_str(e, "vpn_identity")),
            profiles,
        }
    }
}

/// `kind` only when the API states it from verified node metadata; never guessed from a name or port.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Server {
    pub id: String,
    pub country: String,
    pub label: Option<String>,
    pub kind: Option<String>,
    pub path: Vec<String>,
    pub source_label: Option<String>,
    pub variant: Option<String>,
}

fn is_variant(s: &str) -> bool {
    (1..=2).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

/// App API `variant`, or the same rule applied to the server's own name ("Sweden A" -> "A"); else None.
pub fn route_variant(api: Option<&str>, source: Option<&str>) -> Option<String> {
    if let Some(v) = api.filter(|v| is_variant(v)) {
        return Some(v.to_string());
    }
    let words: Vec<&str> = source?.split_whitespace().collect();
    words.last().filter(|w| words.len() >= 2 && is_variant(w)).map(|w| (*w).to_string())
}

impl Server {
    pub fn parse(o: &Value) -> Option<Self> {
        let country = opt_str(o, "country").unwrap_or_default();
        let source = opt_str(o, "label");
        let kind = opt_str(o, "kind").filter(|k| matches!(k.as_str(), "direct" | "alternative" | "cascade"));
        let path = o.get("path").and_then(Value::as_array).map_or(Vec::new(), |a| {
            a.iter().filter_map(Value::as_str).filter(|c| c.len() == 2).map(str::to_string).collect()
        });
        Some(Server {
            id: o.get("id")?.as_str()?.to_string(),
            label: if country.is_empty() { source.clone() } else { None },
            variant: route_variant(opt_str(o, "variant").as_deref(), source.as_deref()),
            country,
            kind,
            path,
            source_label: source,
        })
    }
}

/// A saved route whose id changed keeps following the same route if exactly one new route has the same
/// country, name and stated type (RC6 MarzViewModel.migrateChoice).
pub fn migrate_choice(chosen: &str, old: &[Server], new: &[Server]) -> Option<String> {
    if new.iter().any(|s| s.id == chosen) {
        return None;
    }
    let was = old.iter().find(|s| s.id == chosen)?;
    was.source_label.as_ref()?;
    let same: Vec<&Server> = new
        .iter()
        .filter(|s| s.country == was.country && s.source_label == was.source_label && (was.kind.is_none() || (s.kind == was.kind && s.path == was.path)))
        .collect();
    match same.as_slice() {
        [one] => Some(one.id.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn pro_json(used: i64, limited: bool, stale: bool) -> Value {
        json!({"kind": "pro", "tier": "pro", "ends_at": 9000, "quota_bytes": null, "used_bytes": null, "pro": {
            "used_bytes": used, "limit_bytes": 300_000_000_000i64, "remaining_full_speed_bytes": (300_000_000_000i64 - used).max(0),
            "speed_mode": if limited { "throttled" } else { "full" }, "limited": limited, "normal_speed_limit_bps": if limited { json!(2_000_000) } else { json!(null) },
            "package_start": 100, "package_end": 2000, "next_package_at": 2000, "plan_end": 9000, "updated_at": 50, "stale": stale}})
    }

    #[test]
    fn pro_package_is_read_and_never_becomes_a_quota() {
        let e = Entitlement::parse(&pro_json(100_000_000_000, false, false));
        assert_eq!((e.kind.as_str(), e.quota_bytes, e.used_bytes), ("pro", None, None));
        let p = e.pro.unwrap();
        assert_eq!((p.limit_bytes, p.remaining_full_speed_bytes, p.limited, p.normal_speed_limit_bps, p.next_package_at, p.plan_end, p.stale), (300_000_000_000, 200_000_000_000, false, None, Some(2000), 9000, false));
        let l = Entitlement::parse(&pro_json(300_000_000_000, true, false)).pro.unwrap();
        assert_eq!((l.remaining_full_speed_bytes, l.limited, l.normal_speed_limit_bps), (0, true, Some(2_000_000)));
        assert!(Entitlement::parse(&pro_json(1, false, true)).pro.unwrap().stale);
    }

    #[test]
    fn pro_numbers_that_do_not_add_up_are_not_shown() {
        for (path, v) in [("/pro/limit_bytes", json!(0)), ("/pro/remaining_full_speed_bytes", json!(5)), ("/pro/limited", json!("yes")), ("/pro/plan_end", json!(null)), ("/pro/next_package_at", json!(99999)), ("/pro", json!(null))] {
            let mut j = pro_json(1000, false, false);
            *j.pointer_mut(path).unwrap() = v;
            let e = Entitlement::parse(&j);
            assert_eq!((e.kind.as_str(), e.pro.is_none()), ("pro", true), "{path}");
        }
        let mut j = pro_json(300_000_000_000, true, false);
        j["pro"]["normal_speed_limit_bps"] = json!(null);
        assert!(Entitlement::parse(&j).pro.is_none(), "limited without its speed");
        let mut j = pro_json(10, false, false);
        j["pro"].as_object_mut().unwrap().remove("stale");
        assert!(Entitlement::parse(&j).pro.unwrap().stale, "unknown freshness counts as stale");
    }

    #[test]
    fn plan_being_verified_and_previous_plan() {
        let v = Entitlement::parse(&json!({"kind": "subscription", "tier": "verifying", "ends_at": 5, "quota_bytes": null}));
        assert_eq!((v.kind.as_str(), v.quota_bytes, v.pro), ("verifying", None, None));
        let u = Entitlement::parse(&json!({"kind": "unlimited", "tier": "legacy_unlimited", "ends_at": 5, "quota_bytes": null}));
        assert_eq!((u.kind.as_str(), u.quota_bytes, u.ends_at), ("unlimited", None, 5));
    }

    #[test]
    fn unlimited_never_shows_numbers() {
        let e = Entitlement::parse(&json!({"kind": "unlimited", "ends_at": 5, "quota_bytes": null, "used_bytes": null}));
        assert_eq!((e.kind.as_str(), e.quota_bytes, e.used_bytes), ("unlimited", None, None));
    }

    #[test]
    fn basic_without_a_quota_is_not_called_basic() {
        assert_eq!(Entitlement::parse(&json!({"kind": "basic", "ends_at": 5, "quota_bytes": null})).kind, "other");
        assert_eq!(Entitlement::parse(&json!({"kind": "family_vip", "ends_at": 5})).kind, "other");
    }

    #[test]
    fn bonus_needs_a_positive_reserve() {
        let e = Entitlement::parse(&json!({"kind": "trial", "ends_at": 1, "quota_bytes": 3, "used_bytes": 1, "bonus": {"reserve_bytes": 0}}));
        assert_eq!(e.bonus_bytes, None);
        let e = Entitlement::parse(&json!({"kind": "trial", "ends_at": 1, "quota_bytes": 3, "bonus": {"reserve_bytes": 9}}));
        assert_eq!((e.bonus_bytes, e.used_bytes), (Some(9), Some(0)));
    }

    #[test]
    fn subscription_connects_only_when_active() {
        for (state, can) in [("active", true), ("limited", false), ("expired", false), ("suspended", false), ("none", false)] {
            assert_eq!(Subscription::parse(&json!({"state": state})).can_connect, can);
        }
        assert_eq!(Subscription::parse(&json!({})).state, "none");
    }

    #[test]
    fn profiles_are_read() {
        let s = Subscription::parse(&json!({"state": "active",
            "effective": {"vpn_identity": "vpn_1", "entitlement": {"kind": "basic", "quota_bytes": 10, "used_bytes": 1, "ends_at": 2}},
            "all": [{"vpn_identity": "vpn_1", "entitlement": {"kind": "basic", "quota_bytes": 10, "state": "active", "queued": [{}]}}, {"vpn_identity": ""}]}));
        assert_eq!(s.effective_id.as_deref(), Some("vpn_1"));
        assert_eq!(s.profiles.len(), 1);
        assert_eq!(s.profiles[0].queued, 1);
        assert_eq!(s.entitlement.unwrap().kind, "basic");
    }

    #[test]
    fn route_fields_follow_android() {
        let s = Server::parse(&json!({"id": "abc", "country": "SE", "label": "Sweden A"})).unwrap();
        assert_eq!((s.variant.as_deref(), s.label, s.kind), (Some("A"), None, None));
        let c = Server::parse(&json!({"id": "x", "country": "US", "label": "USA Alternative", "kind": "cascade", "path": ["RU", "US", "bad"]})).unwrap();
        assert_eq!(c.path, ["RU", "US"]);
        assert_eq!(c.variant, None);
        let unknown = Server::parse(&json!({"id": "y", "country": null, "label": "Somewhere", "kind": "guess"})).unwrap();
        assert_eq!((unknown.label.as_deref(), unknown.kind), (Some("Somewhere"), None));
    }

    #[test]
    fn variant_rule() {
        assert_eq!(route_variant(Some("A"), None).as_deref(), Some("A"));
        assert_eq!(route_variant(Some("abc"), Some("Sweden L")).as_deref(), Some("L"));
        assert_eq!(route_variant(None, Some("Sweden")), None);
        assert_eq!(route_variant(None, Some("USA Cascade")), None);
    }

    fn srv(id: &str, c: &str, label: &str) -> Server {
        Server { id: id.into(), country: c.into(), label: None, kind: None, path: vec![], source_label: Some(label.into()), variant: None }
    }

    #[test]
    fn choice_follows_a_renamed_route_only_when_unambiguous() {
        let old = [srv("a", "SE", "Sweden A")];
        assert_eq!(migrate_choice("a", &old, &[srv("b", "SE", "Sweden A")]).as_deref(), Some("b"));
        assert_eq!(migrate_choice("a", &old, &[srv("a", "SE", "Sweden A")]), None);
        assert_eq!(migrate_choice("a", &old, &[srv("b", "SE", "Sweden A"), srv("c", "SE", "Sweden A")]), None);
        assert_eq!(migrate_choice("a", &old, &[srv("b", "US", "Sweden A")]), None);
    }
}
