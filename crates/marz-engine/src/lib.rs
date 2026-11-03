//! What the unprivileged app may ask the privileged helper to run, and the only sing-box config the
//! helper will ever start. The App API's config is untrusted input here: the helper runs as root, so a
//! raw sing-box JSON (which can write files, open listeners or run anything a feature offers) never
//! crosses the boundary. Only typed VLESS + REALITY routes do, each field checked against a strict shape.
pub mod auth;
pub mod ipc;
pub mod killswitch;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const AUTO_TAG: &str = "auto";
pub const SELECTOR_TAG: &str = "proxy";
pub const MAX_ROUTES: usize = 64;
/// Same fixed values sing-tun defaults to, stated so crash cleanup knows exactly what is ours.
pub const ROUTE_TABLE: u32 = 2022;
pub const RULE_INDEX: u32 = 9000;
pub const RULE_INDEX_END: u32 = 9010;
pub const TUN_NAME: &str = "marzvpn0";
/// Same check URL as Android RC6 (ConfigInstaller.URL_TEST_URL).
pub const URL_TEST_URL: &str = "http://captive.apple.com/hotspot-detect.html";
/// Tailscale's address ranges stay outside the tunnel (the owner's SSH path must survive connecting).
const TAILNET: [&str; 2] = ["100.64.0.0/10", "fd7a:115c:a1e0::/48"];

const FLOWS: [&str; 2] = ["", "xtls-rprx-vision"];
const FINGERPRINTS: [&str; 10] = ["chrome", "firefox", "safari", "ios", "android", "edge", "360", "qq", "random", "randomized"];

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    pub tag: String,
    pub server: String,
    pub port: u16,
    pub uuid: String,
    pub flow: String,
    pub server_name: String,
    pub fingerprint: String,
    pub public_key: String,
    pub short_id: String,
}

/// Debug output never contains the address, UUID or keys, so no `{:?}` can leak them into a log.
impl std::fmt::Debug for Route {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Route({})", self.tag)
    }
}

/// `selected`: [AUTO_TAG] or the tag of one of `routes`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineRequest {
    pub routes: Vec<Route>,
    pub selected: String,
}

/// One-line summary for diagnostics, without credentials.
pub fn describe(req: &EngineRequest) -> String {
    format!("routes={} selected={}", req.routes.len(), req.selected)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigError(pub &'static str);

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for ConfigError {}

fn is_tag(s: &str) -> bool {
    let Some(rest) = s.strip_prefix("srv-") else { return false };
    (1..=64).contains(&rest.len()) && rest.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn is_dns_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 253
        && !s.starts_with('-')
        && !s.starts_with('.')
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
}

fn is_host(s: &str) -> bool {
    if s.parse::<std::net::IpAddr>().is_ok() {
        return true;
    }
    is_dns_name(s) && !s.contains("..")
}

fn is_uuid(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => *c == b'-',
            _ => c.is_ascii_hexdigit(),
        })
}

fn is_reality_key(s: &str) -> bool {
    s.len() == 43 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn is_short_id(s: &str) -> bool {
    s.len() <= 16 && s.len() % 2 == 0 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

impl Route {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !is_tag(&self.tag) {
            return Err(ConfigError("invalid_tag"));
        }
        if !is_host(&self.server) {
            return Err(ConfigError("invalid_server"));
        }
        if self.port == 0 {
            return Err(ConfigError("invalid_port"));
        }
        if !is_uuid(&self.uuid) {
            return Err(ConfigError("invalid_uuid"));
        }
        if !FLOWS.contains(&self.flow.as_str()) {
            return Err(ConfigError("invalid_flow"));
        }
        if !is_dns_name(&self.server_name) {
            return Err(ConfigError("invalid_server_name"));
        }
        if !FINGERPRINTS.contains(&self.fingerprint.as_str()) {
            return Err(ConfigError("invalid_fingerprint"));
        }
        if !is_reality_key(&self.public_key) {
            return Err(ConfigError("invalid_public_key"));
        }
        if !is_short_id(&self.short_id) {
            return Err(ConfigError("invalid_short_id"));
        }
        Ok(())
    }

    fn outbound(&self) -> Value {
        let mut o = json!({
            "type": "vless", "tag": self.tag, "server": self.server, "server_port": self.port,
            "uuid": self.uuid, "flow": self.flow,
            "tls": {
                "enabled": true, "server_name": self.server_name,
                "utls": {"enabled": true, "fingerprint": self.fingerprint},
                "reality": {"enabled": true, "public_key": self.public_key, "short_id": self.short_id},
            },
        });
        if !self.flow.is_empty() {
            o["packet_encoding"] = json!("xudp");
        }
        o
    }
}

impl EngineRequest {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.routes.is_empty() {
            return Err(ConfigError("no_routes"));
        }
        if self.routes.len() > MAX_ROUTES {
            return Err(ConfigError("too_many_routes"));
        }
        let mut tags = std::collections::HashSet::new();
        for r in &self.routes {
            r.validate()?;
            if !tags.insert(r.tag.as_str()) {
                return Err(ConfigError("duplicate_tag"));
            }
        }
        if self.selected != AUTO_TAG && !tags.contains(self.selected.as_str()) {
            return Err(ConfigError("unknown_selection"));
        }
        Ok(())
    }

    /// Takes the VLESS + REALITY routes and the selection out of `GET /v1/vpn/config`, nothing else.
    /// A route of any other shape fails the whole config: a partly understood list would silently hide
    /// routes the user is paying for, and the server promises to send only this shape.
    pub fn from_api_config(text: &str) -> Result<Self, ConfigError> {
        let v: Value = serde_json::from_str(text).map_err(|_| ConfigError("invalid_json"))?;
        let outbounds = v.get("outbounds").and_then(Value::as_array).ok_or(ConfigError("no_outbounds"))?;
        let mut routes = Vec::new();
        let mut selected = AUTO_TAG.to_string();
        for o in outbounds {
            match o.get("type").and_then(Value::as_str) {
                Some("vless") => routes.push(route_from(o)?),
                Some("selector") if o.get("tag").and_then(Value::as_str) == Some(SELECTOR_TAG) => {
                    if let Some(d) = o.get("default").and_then(Value::as_str) {
                        selected = d.to_string();
                    }
                }
                Some("selector" | "urltest" | "direct") => {}
                _ => return Err(ConfigError("unsupported_outbound")),
            }
        }
        let req = EngineRequest { routes, selected };
        req.validate()?;
        Ok(req)
    }

    /// The config without the values Marzban picks at random for each link (same rule as RC6
    /// ConfigInstaller.material): a difference only there is no reason to restart the tunnel.
    pub fn material(&self) -> Vec<(String, String, u16, String, String, String, String)> {
        let mut m: Vec<_> = self
            .routes
            .iter()
            .map(|r| (r.tag.clone(), r.server.clone(), r.port, r.uuid.clone(), r.flow.clone(), r.fingerprint.clone(), r.public_key.clone()))
            .collect();
        m.sort();
        m
    }
}

fn str_field<'a>(o: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut cur = o;
    for p in path {
        cur = cur.get(p)?;
    }
    cur.as_str()
}

fn route_from(o: &Value) -> Result<Route, ConfigError> {
    let bad = ConfigError("invalid_route");
    if o.pointer("/tls/reality/enabled").and_then(Value::as_bool) != Some(true) {
        return Err(ConfigError("not_reality"));
    }
    let r = Route {
        tag: str_field(o, &["tag"]).ok_or(bad)?.to_string(),
        server: str_field(o, &["server"]).ok_or(bad)?.to_string(),
        port: o.get("server_port").and_then(Value::as_u64).and_then(|p| u16::try_from(p).ok()).ok_or(bad)?,
        uuid: str_field(o, &["uuid"]).ok_or(bad)?.to_string(),
        flow: str_field(o, &["flow"]).unwrap_or("").to_string(),
        server_name: str_field(o, &["tls", "server_name"]).ok_or(bad)?.to_string(),
        fingerprint: str_field(o, &["tls", "utls", "fingerprint"]).unwrap_or("chrome").to_string(),
        public_key: str_field(o, &["tls", "reality", "public_key"]).ok_or(bad)?.to_string(),
        short_id: str_field(o, &["tls", "reality", "short_id"]).unwrap_or("").to_string(),
    };
    r.validate()?;
    Ok(r)
}

/// Where the helper reaches the engine: a loopback controller with a one-run random secret.
#[derive(Debug, Clone)]
pub struct Controller {
    pub port: u16,
    pub secret: String,
}

/// The full sing-box 1.14 config: DNS through the tunnel as in the App API's `build_config`, with the RC6
/// URL-test defaults, a controller for route switching and the fixed routing indexes. Differs from the API's
/// config on purpose: no direct outbound and no system resolver, because the kill switch allows neither.
/// Call only with a request that passed [EngineRequest::validate] and [killswitch::pin_routes].
pub fn build_config(req: &EngineRequest, controller: &Controller) -> Value {
    let tags: Vec<&str> = req.routes.iter().map(|r| r.tag.as_str()).collect();
    let mut selector_members = vec![AUTO_TAG];
    selector_members.extend(&tags);
    let mut outbounds = vec![
        json!({"type": "selector", "tag": SELECTOR_TAG, "outbounds": selector_members, "default": req.selected,
               "interrupt_exist_connections": true}),
        json!({"type": "urltest", "tag": AUTO_TAG, "outbounds": tags, "url": URL_TEST_URL, "interval": "10m"}),
    ];
    outbounds.extend(req.routes.iter().map(Route::outbound));
    let mut config = json!({
        "log": {"level": "error", "timestamp": false},
        "dns": {
            "servers": [
                {"type": "https", "tag": "remote", "server": "1.1.1.1", "detour": SELECTOR_TAG},
            ],
            "final": "remote",
        },
        "inbounds": [{
            "type": "tun", "tag": "tun-in", "interface_name": TUN_NAME,
            "address": ["172.19.0.1/30", "fdfe:dcba:9876::1/126"],
            "auto_route": true, "strict_route": true, "stack": "mixed",
            "iproute2_table_index": ROUTE_TABLE, "iproute2_rule_index": RULE_INDEX,
            "route_exclude_address": TAILNET,
        }],
        "outbounds": outbounds,
        "route": {
            "rules": [
                {"action": "sniff"},
                {"protocol": "dns", "action": "hijack-dns"},
                // Would need a way out of the tunnel, which the firewall refuses anyway: fail at once.
                {"ip_is_private": true, "action": "reject"},
            ],
            "final": SELECTOR_TAG,
            "auto_detect_interface": true,
        },
        "experimental": {"clash_api": {
            "external_controller": format!("127.0.0.1:{}", controller.port),
            "secret": controller.secret,
        }},
    });
    // How the Linux firewall recognises the engine. Windows goes by the program's path instead (wfp_plan).
    if cfg!(target_os = "linux") {
        config["route"]["default_mark"] = json!(killswitch::ENGINE_MARK);
    }
    config
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(tag: &str) -> Route {
        Route {
            tag: tag.into(),
            server: "203.0.113.7".into(),
            port: 443,
            uuid: "00000000-0000-4000-8000-000000000001".into(),
            flow: "xtls-rprx-vision".into(),
            server_name: "www.example.com".into(),
            fingerprint: "chrome".into(),
            public_key: "A".repeat(43),
            short_id: "abcd".into(),
        }
    }

    fn req() -> EngineRequest {
        EngineRequest { routes: vec![route("srv-aaa"), route("srv-bbb")], selected: "srv-bbb".into() }
    }

    #[test]
    fn valid_request_passes() {
        req().validate().unwrap();
    }

    #[test]
    fn hostile_fields_are_refused() {
        type Change = fn(&mut Route);
        let cases: Vec<(Change, &str)> = vec![
            (|r| r.tag = "srv-../../etc".into(), "invalid_tag"),
            (|r| r.tag = "direct".into(), "invalid_tag"),
            (|r| r.server = "evil.com\",\"x\":\"".into(), "invalid_server"),
            (|r| r.server = "a b".into(), "invalid_server"),
            (|r| r.port = 0, "invalid_port"),
            (|r| r.uuid = "not-a-uuid".into(), "invalid_uuid"),
            (|r| r.flow = "xtls-rprx-direct".into(), "invalid_flow"),
            (|r| r.server_name = "x;rm -rf".into(), "invalid_server_name"),
            (|r| r.fingerprint = "$(id)".into(), "invalid_fingerprint"),
            (|r| r.public_key = "short".into(), "invalid_public_key"),
            (|r| r.short_id = "xyz".into(), "invalid_short_id"),
            (|r| r.short_id = "abc".into(), "invalid_short_id"),
        ];
        for (change, code) in cases {
            let mut r = req();
            change(&mut r.routes[0]);
            assert_eq!(r.validate(), Err(ConfigError(code)));
        }
    }

    #[test]
    fn selection_must_be_a_known_route() {
        let mut r = req();
        r.selected = "srv-zzz".into();
        assert_eq!(r.validate(), Err(ConfigError("unknown_selection")));
        r.selected = AUTO_TAG.into();
        r.validate().unwrap();
    }

    #[test]
    fn empty_and_duplicate_lists_are_refused() {
        let mut r = req();
        r.routes[1].tag = "srv-aaa".into();
        r.selected = AUTO_TAG.into();
        assert_eq!(r.validate(), Err(ConfigError("duplicate_tag")));
        r.routes.clear();
        assert_eq!(r.validate(), Err(ConfigError("no_routes")));
    }

    #[test]
    fn unknown_ipc_fields_are_refused() {
        let text = serde_json::to_value(route("srv-a")).unwrap();
        let mut o = text.as_object().unwrap().clone();
        o.insert("experimental".into(), json!({"clash_api": {}}));
        assert!(serde_json::from_value::<Route>(Value::Object(o)).is_err());
    }

    /// Exact shape from the backend's app_api/singbox.py build_config.
    fn api_config(extra: Value) -> String {
        let mut c = json!({
            "log": {"level": "warn"},
            "inbounds": [{"type": "tun", "tag": "tun-in"}],
            "outbounds": [
                {"type": "selector", "tag": "proxy", "outbounds": ["auto", "srv-abc"], "default": "srv-abc"},
                {"type": "urltest", "tag": "auto", "outbounds": ["srv-abc"]},
                {"type": "vless", "tag": "srv-abc", "server": "203.0.113.7", "server_port": 8443,
                 "uuid": "00000000-0000-4000-8000-000000000001", "flow": "xtls-rprx-vision", "packet_encoding": "xudp",
                 "tls": {"enabled": true, "server_name": "www.example.com",
                         "utls": {"enabled": true, "fingerprint": "firefox"},
                         "reality": {"enabled": true, "public_key": "A".repeat(43), "short_id": "ab12"}}},
                {"type": "direct", "tag": "direct"},
            ],
        });
        if let Some(o) = extra.as_object() {
            for (k, v) in o {
                c[k] = v.clone();
            }
        }
        c.to_string()
    }

    #[test]
    fn api_config_is_reduced_to_typed_routes() {
        let hostile = json!({"experimental": {"clash_api": {"external_controller": "0.0.0.0:9090"}},
                             "log": {"output": "/etc/cron.d/x"}});
        let r = EngineRequest::from_api_config(&api_config(hostile)).unwrap();
        assert_eq!(r.selected, "srv-abc");
        assert_eq!(r.routes.len(), 1);
        assert_eq!(r.routes[0].port, 8443);
        assert_eq!(r.routes[0].fingerprint, "firefox");
        let built = build_config(&r, &Controller { port: 1, secret: "s".into() }).to_string();
        assert!(!built.contains("/etc/cron"));
        assert!(!built.contains("0.0.0.0:9090"));
    }

    #[test]
    fn foreign_outbounds_fail_the_config() {
        let bad = json!({"outbounds": [{"type": "shadowsocks", "tag": "x"}]}).to_string();
        assert_eq!(EngineRequest::from_api_config(&bad), Err(ConfigError("unsupported_outbound")));
        let not_reality = json!({"outbounds": [{"type": "vless", "tag": "srv-a", "tls": {"enabled": true}}]}).to_string();
        assert_eq!(EngineRequest::from_api_config(&not_reality), Err(ConfigError("not_reality")));
    }

    #[test]
    fn built_config_keeps_the_dns_and_route_policy() {
        let c = build_config(&req(), &Controller { port: 4242, secret: "s3".into() });
        assert_eq!(c["route"]["final"], "proxy");
        assert_eq!(c["route"]["rules"][1]["action"], "hijack-dns");
        assert_eq!(c["dns"]["final"], "remote");
        assert_eq!(c["dns"]["servers"][0]["detour"], "proxy");
        assert_eq!(c["inbounds"][0]["strict_route"], true);
        assert_eq!(c["outbounds"][0]["default"], "srv-bbb");
        assert_eq!(c["experimental"]["clash_api"]["external_controller"], "127.0.0.1:4242");
    }

    #[test]
    fn random_link_values_are_not_material() {
        let a = req();
        let mut b = req();
        b.routes[0].short_id = "ffff".into();
        b.routes[0].server_name = "other.example.com".into();
        assert_eq!(a.material(), b.material());
        b.routes[0].port = 8443;
        assert_ne!(a.material(), b.material());
    }

    #[test]
    fn describe_has_no_credentials() {
        for d in [describe(&req()), format!("{:?}", req())] {
            assert!(!d.contains("00000000") && !d.contains("203.0.113") && !d.contains("AAAA"));
        }
    }
}
