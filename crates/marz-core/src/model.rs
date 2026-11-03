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

/// `kind`: trial | basic | unlimited | other. Other = older or special profiles: Marzban's own numbers,
/// without inventing a plan name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entitlement {
    pub kind: String,
    pub ends_at: i64,
    pub quota_bytes: Option<i64>,
    pub used_bytes: Option<i64>,
    pub bonus_bytes: Option<i64>,
}

impl Entitlement {
    pub fn parse(e: &Value) -> Self {
        let ends_at = e.get("ends_at").and_then(Value::as_i64).unwrap_or(0);
        let quota = e.get("quota_bytes").and_then(Value::as_i64);
        let used = e.get("used_bytes").and_then(Value::as_i64).unwrap_or(0);
        let bonus = e.pointer("/bonus/reserve_bytes").and_then(Value::as_i64).filter(|b| *b > 0);
        let kind = e.get("kind").and_then(Value::as_str).unwrap_or("");
        match kind {
            "unlimited" => Entitlement { kind: "unlimited".into(), ends_at, quota_bytes: None, used_bytes: None, bonus_bytes: bonus },
            "trial" => Entitlement { kind: "trial".into(), ends_at, quota_bytes: Some(quota.unwrap_or(0)), used_bytes: Some(used), bonus_bytes: bonus },
            "basic" if quota.is_some() => Entitlement { kind: "basic".into(), ends_at, quota_bytes: quota, used_bytes: Some(used), bonus_bytes: bonus },
            "basic" => Entitlement { kind: "other".into(), ends_at, quota_bytes: None, used_bytes: None, bonus_bytes: bonus },
            _ => Entitlement { kind: "other".into(), ends_at, quota_bytes: quota, used_bytes: quota.map(|_| used), bonus_bytes: bonus },
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
