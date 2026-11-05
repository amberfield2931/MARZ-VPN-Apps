//! What the kill switch enforces, as data: the exact nftables table for Linux and the exact filter list
//! for Windows. No I/O here, so every rule can be checked by a test on any machine. The helper applies
//! this; see KILL-SWITCH-DESIGN.md.
//!
//! The rule of the design: **the block exists independently of the engine.** The tables live in the
//! kernel, not in a process, and nothing here depends on the tunnel being up.
use crate::{ConfigError, EngineRequest, Route, TUN_NAME};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::net::IpAddr;

/// The socket mark the engine puts on its own connections (SO_MARK needs CAP_NET_ADMIN: an ordinary
/// process cannot claim it). A mark alone opens nothing: it must also match an endpoint below.
pub const ENGINE_MARK: u32 = 0x4d52;
pub const NFT_TABLE: &str = "marzvpn_ks";
pub const NFT_DNS_TABLE: &str = "marzvpn_dns";

/// The only thing allowed out of a physical interface for VPN traffic: this exact address and TCP port,
/// for sockets of root that carry [ENGINE_MARK]. VLESS + REALITY runs over TCP, so there is no UDP form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Endpoint {
    pub ip: IpAddr,
    pub port: u16,
}

/// An address that could never be a VPN node: refusing it keeps "endpoint" from becoming a way to open
/// loopback, multicast or the whole network with the engine's mark.
pub fn endpoint_address_allowed(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(a) => !(a.is_unspecified() || a.is_loopback() || a.is_multicast() || a.is_broadcast() || a.is_link_local()),
        IpAddr::V6(a) => !(a.is_unspecified() || a.is_loopback() || a.is_multicast() || (a.segments()[0] & 0xffc0) == 0xfe80),
    }
}

/// Endpoints of a request whose servers are all addresses (see [pin_routes]), sorted and without repeats.
pub fn endpoints_of(req: &EngineRequest) -> Result<Vec<Endpoint>, ConfigError> {
    let mut out = Vec::new();
    for r in &req.routes {
        let ip: IpAddr = r.server.parse().map_err(|_| ConfigError("endpoint_unresolved"))?;
        if !endpoint_address_allowed(ip) {
            return Err(ConfigError("endpoint_refused"));
        }
        out.push(Endpoint { ip, port: r.port });
    }
    out.sort();
    out.dedup();
    Ok(out)
}

/// Replaces server names with addresses, so the engine never needs DNS to find a node and the firewall
/// knows exactly what to allow. `resolve` returns None when a name has no usable answer; such a route is
/// left out (a route that cannot be allowed through the firewall can never carry traffic). No route left
/// is an error: the caller refuses to connect rather than open anything.
pub fn pin_routes(req: &EngineRequest, mut resolve: impl FnMut(&str) -> Option<IpAddr>) -> Result<EngineRequest, ConfigError> {
    let mut routes: Vec<Route> = Vec::with_capacity(req.routes.len());
    for r in &req.routes {
        let ip = match r.server.parse::<IpAddr>() {
            Ok(ip) => Some(ip),
            Err(_) => resolve(&r.server),
        };
        if let Some(ip) = ip {
            let mut pinned = r.clone();
            pinned.server = ip.to_string();
            routes.push(pinned);
        }
    }
    if routes.is_empty() {
        return Err(ConfigError("endpoint_unresolved"));
    }
    let selected = if routes.iter().any(|r| r.tag == req.selected) { req.selected.clone() } else { crate::AUTO_TAG.to_string() };
    Ok(EngineRequest { routes, selected })
}

/// Saved on disk while the switch is ON. Holds no credentials: "protect after a restart" needs only the
/// fact itself, plus the addresses of named servers so a reconnect while blocked needs no DNS.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArmedState {
    pub version: u32,
    pub pins: BTreeMap<String, IpAddr>,
    /// The user who turned the VPN on (see crate::auth). Not a credential.
    #[serde(default)]
    pub owner: Option<String>,
}

pub const ARMED_VERSION: u32 = 1;

fn element_lines(endpoints: &[Endpoint], v6: bool) -> String {
    endpoints
        .iter()
        .filter(|e| e.ip.is_ipv6() == v6)
        .map(|e| format!("{} . {}", e.ip, e.port))
        .collect::<Vec<_>>()
        .join(", ")
}

fn set_block(name: &str, ty: &str, elements: &str) -> String {
    let elements = if elements.is_empty() { String::new() } else { format!("\n\t\telements = {{ {elements} }}") };
    format!("\tset {name} {{\n\t\ttype {ty} . inet_service{elements}\n\t}}\n")
}

/// One nft transaction, applied by the kernel all at once or not at all: there is no moment with the old
/// table gone and the new one missing. It names only our table, so other firewalls are never touched.
///
/// Deliberately absent: `ct state established accept` (connections opened before the VPN would survive),
/// any port-based exception for 53/853/443, LAN ranges, Tailscale. DHCP and neighbour discovery are the
/// whole list of network upkeep, because without them the network itself disappears.
pub fn nft_script(endpoints: &[Endpoint]) -> String {
    let t = NFT_TABLE;
    let tun = TUN_NAME;
    let mark = ENGINE_MARK;
    let ep4 = set_block("ep4", "ipv4_addr", &element_lines(endpoints, false));
    let ep6 = set_block("ep6", "ipv6_addr", &element_lines(endpoints, true));
    format!(
        "table inet {t} {{}}\n\
         delete table inet {t}\n\
         table inet {t} {{\n\
         {ep4}{ep6}\
         \tchain output {{\n\
         \t\ttype filter hook output priority filter; policy drop;\n\
         \t\toifname \"lo\" accept\n\
         \t\toifname \"{tun}\" accept\n\
         \t\tmeta mark {mark:#x} meta skuid 0 ip daddr . tcp dport @ep4 accept\n\
         \t\tmeta mark {mark:#x} meta skuid 0 ip6 daddr . tcp dport @ep6 accept\n\
         \t\tmeta nfproto ipv4 udp sport 68 udp dport 67 accept\n\
         \t\tip6 saddr {{ fe80::/10, :: }} ip6 daddr ff02::1:2 udp sport 546 udp dport 547 accept\n\
         \t\tip6 hoplimit 255 icmpv6 type {{ nd-router-solicit, nd-neighbor-solicit, nd-neighbor-advert }} accept\n\
         \t\tip6 hoplimit 1 ip6 saddr {{ fe80::/10, :: }} icmpv6 type mld2-listener-report accept\n\
         \t\tcounter name \"blocked\" reject with icmpx type admin-prohibited\n\
         \t}}\n\
         \tcounter blocked {{\n\t}}\n\
         \tchain forward {{\n\
         \t\ttype filter hook forward priority filter; policy drop;\n\
         \t\toifname \"{tun}\" accept\n\
         \t\tiifname \"{tun}\" ct state established,related accept\n\
         \t}}\n\
         }}\n"
    )
}

/// With the Kill Switch setting OFF the computer may go direct when the VPN drops, but while the engine runs
/// DNS must still go only through the tunnel: systemd-resolved was measured sending some queries to the
/// physical link's resolver right after connecting, past the routes. Only DNS and DNS over TLS are refused
/// here; everything else keeps its normal path. Exists only while an engine does.
pub fn nft_dns_guard_script() -> String {
    format!(
        "table inet {t} {{}}\n\
         delete table inet {t}\n\
         table inet {t} {{\n\
         \tchain output {{\n\
         \t\ttype filter hook output priority filter; policy accept;\n\
         \t\toifname {{ \"lo\", \"{tun}\" }} accept\n\
         \t\tmeta mark {mark:#x} accept\n\
         \t\tmeta l4proto {{ tcp, udp }} th dport {{ 53, 853 }} reject with icmpx type admin-prohibited\n\
         \t}}\n\
         }}\n",
        t = NFT_DNS_TABLE,
        tun = TUN_NAME,
        mark = ENGINE_MARK
    )
}

pub fn nft_dns_guard_remove_script() -> String {
    format!("table inet {t} {{}}\ndelete table inet {t}\n", t = NFT_DNS_TABLE)
}

/// Removes our table and nothing else. The empty declaration first makes it succeed when it is absent.
pub fn nft_remove_script() -> String {
    format!("table inet {t} {{}}\ndelete table inet {t}\n", t = NFT_TABLE)
}

/// The listing still has what makes the table a block. Any change an outside tool makes to these lines
/// (or removal of the table) makes the helper apply the table again.
pub fn nft_listing_intact(listing: &str) -> bool {
    let compact: String = listing.split_whitespace().collect::<Vec<_>>().join(" ");
    compact.contains(&format!("table inet {NFT_TABLE} "))
        && compact.contains("hook output priority filter; policy drop;")
        && compact.contains("hook forward priority filter; policy drop;")
        && compact.contains(&format!("oifname \"{TUN_NAME}\" accept"))
        && compact.contains("reject with icmpx admin-prohibited")
}

// ---- Windows -------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WfpLayer {
    /// Outgoing connections and first UDP packets (ALE_AUTH_CONNECT).
    Connect4,
    Connect6,
    /// Incoming connections and first packets from outside (ALE_AUTH_RECV_ACCEPT).
    Accept4,
    Accept6,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WfpMatch {
    /// Everything on the layer.
    All,
    Loopback,
    /// The tunnel adapter, when it exists: its LUID is only known after the engine created it.
    TunAdapter,
    /// The installed sing-box.exe talking TCP to exactly this address and port.
    EngineTo(Endpoint),
    DhcpV4,
    DhcpV6,
    /// ICMPv6 neighbour discovery, types as in the Linux table.
    NeighbourDiscovery(u8),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WfpFilter {
    pub layer: WfpLayer,
    pub matches: WfpMatch,
    pub permit: bool,
    /// Higher wins inside our sublayer. A block at weight 0 under every permit, so the set means
    /// "everything is blocked except".
    pub weight: u8,
}

/// The whole Windows filter set. Pure, so its logic has real tests even though the kernel side has only
/// been cross-compiled.
pub fn wfp_plan(endpoints: &[Endpoint], tun_present: bool) -> Vec<WfpFilter> {
    use WfpLayer::*;
    let mut f = Vec::new();
    for layer in [Connect4, Connect6, Accept4, Accept6] {
        f.push(WfpFilter { layer, matches: WfpMatch::All, permit: false, weight: 0 });
        f.push(WfpFilter { layer, matches: WfpMatch::Loopback, permit: true, weight: 10 });
        if tun_present {
            f.push(WfpFilter { layer, matches: WfpMatch::TunAdapter, permit: true, weight: 10 });
        }
    }
    for e in endpoints {
        let layer = if e.ip.is_ipv6() { Connect6 } else { Connect4 };
        f.push(WfpFilter { layer, matches: WfpMatch::EngineTo(*e), permit: true, weight: 10 });
    }
    f.push(WfpFilter { layer: Connect4, matches: WfpMatch::DhcpV4, permit: true, weight: 10 });
    f.push(WfpFilter { layer: Connect6, matches: WfpMatch::DhcpV6, permit: true, weight: 10 });
    for t in [133u8, 135, 136] {
        f.push(WfpFilter { layer: Connect6, matches: WfpMatch::NeighbourDiscovery(t), permit: true, weight: 10 });
    }
    // Answers from routers and neighbours arrive as new flows on the accept layer.
    for t in [134u8, 135, 136] {
        f.push(WfpFilter { layer: Accept6, matches: WfpMatch::NeighbourDiscovery(t), permit: true, weight: 10 });
    }
    f.push(WfpFilter { layer: Accept4, matches: WfpMatch::DhcpV4, permit: true, weight: 10 });
    f.push(WfpFilter { layer: Accept6, matches: WfpMatch::DhcpV6, permit: true, weight: 10 });
    f
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{build_config, Controller};

    fn ep(ip: &str, port: u16) -> Endpoint {
        Endpoint { ip: ip.parse().unwrap(), port }
    }

    fn route(tag: &str, server: &str, port: u16) -> Route {
        Route {
            tag: tag.into(),
            server: server.into(),
            port,
            uuid: "00000000-0000-4000-8000-000000000001".into(),
            flow: "xtls-rprx-vision".into(),
            server_name: "www.example.com".into(),
            fingerprint: "chrome".into(),
            public_key: "A".repeat(43),
            short_id: "abcd".into(),
        }
    }

    fn output_chain(script: &str) -> String {
        let start = script.find("chain output").unwrap();
        let end = script[start..].find("\n\t}").unwrap();
        script[start..start + end].to_string()
    }

    #[test]
    fn table_is_replaced_in_one_transaction_and_names_only_itself() {
        let s = nft_script(&[]);
        assert!(s.starts_with("table inet marzvpn_ks {}\ndelete table inet marzvpn_ks\ntable inet marzvpn_ks {"));
        for forbidden in ["flush ruleset", "flush table", "table ip ", "table ip6 ", "delete table inet filter"] {
            assert!(!s.contains(forbidden), "{forbidden}");
        }
        assert!(!nft_remove_script().contains("ruleset"));
    }

    #[test]
    fn nothing_but_the_stated_exceptions_is_accepted() {
        let s = nft_script(&[ep("203.0.113.7", 443), ep("2001:db8::7", 8443)]);
        let chain = output_chain(&s);
        assert!(chain.contains("policy drop"));
        let accepts: Vec<&str> = chain.lines().filter(|l| l.contains("accept")).collect();
        assert_eq!(accepts.len(), 8, "{chain}");
        for line in chain.lines() {
            for hole in ["ct state", "dport 53", "dport 853", "dport 443", "192.168.", "10.0.0.0", "tailscale", "skgid"] {
                assert!(!line.contains(hole), "{hole} in {line}");
            }
        }
        for line in &accepts {
            let upkeep = line.contains("sport 68") || line.contains("sport 546") || line.contains("icmpv6");
            let tunnel = line.contains("oifname \"lo\"") || line.contains("oifname \"marzvpn0\"");
            let engine = line.contains("meta mark 0x4d52 meta skuid 0") && line.contains("tcp dport @ep");
            assert!(upkeep || tunnel || engine, "{line}");
        }
    }

    #[test]
    fn endpoints_are_exact_address_and_port() {
        let s = nft_script(&[ep("203.0.113.7", 443), ep("203.0.113.8", 8443), ep("2001:db8::7", 443)]);
        assert!(s.contains("elements = { 203.0.113.7 . 443, 203.0.113.8 . 8443 }"));
        assert!(s.contains("elements = { 2001:db8::7 . 443 }"));
        let empty = nft_script(&[]);
        assert!(!empty.contains("elements"));
    }

    #[test]
    fn forward_chain_lets_nothing_reach_a_physical_interface() {
        let s = nft_script(&[]);
        let f = &s[s.find("chain forward").unwrap()..];
        assert!(f.contains("policy drop"));
        assert_eq!(f.lines().filter(|l| l.contains("accept")).count(), 2);
        assert!(f.contains("oifname \"marzvpn0\" accept"));
        assert!(f.contains("iifname \"marzvpn0\" ct state established,related accept"));
    }

    #[test]
    fn dns_guard_refuses_only_dns_outside_the_tunnel() {
        let s = nft_dns_guard_script();
        assert!(s.contains("policy accept"));
        assert!(s.contains("oifname { \"lo\", \"marzvpn0\" } accept"));
        assert!(s.contains("th dport { 53, 853 } reject"));
        assert!(!s.contains("ruleset") && !s.contains("marzvpn_ks"));
        assert!(nft_dns_guard_remove_script().contains("delete table inet marzvpn_dns"));
    }

    #[test]
    fn listing_check_notices_a_changed_table() {
        let good = "table inet marzvpn_ks {\n chain output {\n type filter hook output priority filter; policy drop;\n oifname \"marzvpn0\" accept\n reject with icmpx admin-prohibited\n }\n chain forward {\n type filter hook forward priority filter; policy drop;\n }\n}\n";
        assert!(nft_listing_intact(good));
        assert!(!nft_listing_intact(""));
        assert!(!nft_listing_intact(&good.replace("hook output priority filter; policy drop;", "hook output priority filter; policy accept;")));
    }

    #[test]
    fn useless_addresses_are_refused_as_endpoints() {
        for bad in ["0.0.0.0", "127.0.0.1", "224.0.0.1", "255.255.255.255", "169.254.1.1", "::", "::1", "ff02::1", "fe80::1"] {
            assert!(!endpoint_address_allowed(bad.parse().unwrap()), "{bad}");
        }
        for good in ["203.0.113.7", "10.77.0.1", "2001:db8::1"] {
            assert!(endpoint_address_allowed(good.parse().unwrap()), "{good}");
        }
    }

    #[test]
    fn names_are_pinned_and_the_config_has_no_name_to_resolve() {
        let req = EngineRequest { routes: vec![route("srv-a", "node.example.com", 443), route("srv-b", "203.0.113.9", 8443)], selected: "srv-a".into() };
        let pinned = pin_routes(&req, |h| (h == "node.example.com").then(|| "203.0.113.8".parse().unwrap())).unwrap();
        assert_eq!(pinned.routes[0].server, "203.0.113.8");
        assert_eq!(pinned.routes[0].server_name, "www.example.com");
        assert_eq!(pinned.selected, "srv-a");
        assert_eq!(endpoints_of(&pinned).unwrap(), vec![ep("203.0.113.8", 443), ep("203.0.113.9", 8443)]);
    }

    #[test]
    fn a_route_without_an_address_is_dropped_and_no_routes_is_an_error() {
        let req = EngineRequest { routes: vec![route("srv-a", "node.example.com", 443), route("srv-b", "203.0.113.9", 8443)], selected: "srv-a".into() };
        let pinned = pin_routes(&req, |_| None).unwrap();
        assert_eq!(pinned.routes.len(), 1);
        assert_eq!(pinned.selected, crate::AUTO_TAG);
        let only = EngineRequest { routes: vec![route("srv-a", "node.example.com", 443)], selected: "auto".into() };
        assert_eq!(pin_routes(&only, |_| None), Err(ConfigError("endpoint_unresolved")));
        assert_eq!(endpoints_of(&only), Err(ConfigError("endpoint_unresolved")));
        let bad = EngineRequest { routes: vec![route("srv-a", "127.0.0.1", 443)], selected: "auto".into() };
        assert_eq!(endpoints_of(&bad), Err(ConfigError("endpoint_refused")));
    }

    /// Nothing in the engine's config can go around the tunnel: no direct outbound, no system resolver,
    /// no private-range exception, and its own connections carry the mark the firewall expects.
    #[test]
    fn engine_config_has_no_way_around_the_tunnel() {
        let req = EngineRequest { routes: vec![route("srv-a", "203.0.113.9", 443)], selected: "srv-a".into() };
        let c = build_config(&req, &Controller { port: 1, secret: "s".into() });
        let text = c.to_string();
        assert!(!text.contains("\"direct\""), "{text}");
        assert!(!text.contains("\"local\""), "{text}");
        assert!(!text.contains("default_domain_resolver"), "{text}");
        // The mark exists on Linux only; on Windows the WFP filters tell the engine by its program path.
        if cfg!(target_os = "linux") {
            assert_eq!(c["route"]["default_mark"], ENGINE_MARK);
        }
        assert_eq!(c["inbounds"][0]["strict_route"], true);
        let rules = c["route"]["rules"].as_array().unwrap();
        assert_eq!(rules[1]["action"], "hijack-dns");
        assert_eq!(rules[2]["action"], "reject");
        assert_eq!(c["dns"]["servers"].as_array().unwrap().len(), 1);
        assert_eq!(c["dns"]["servers"][0]["detour"], "proxy");
    }

    #[test]
    fn armed_state_cannot_hold_credentials() {
        let mut s = ArmedState { version: ARMED_VERSION, ..Default::default() };
        s.pins.insert("node.example.com".into(), "203.0.113.8".parse().unwrap());
        let text = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<ArmedState>(&text).unwrap(), s);
        assert!(serde_json::from_str::<ArmedState>(r#"{"version":1,"pins":{},"uuid":"x"}"#).is_err());
    }

    #[test]
    fn windows_plan_blocks_everything_but_the_stated_exceptions() {
        let plan = wfp_plan(&[ep("203.0.113.7", 443), ep("2001:db8::7", 443)], false);
        for layer in [WfpLayer::Connect4, WfpLayer::Connect6, WfpLayer::Accept4, WfpLayer::Accept6] {
            let block = plan.iter().find(|f| f.layer == layer && f.matches == WfpMatch::All).unwrap();
            assert!(!block.permit);
            assert!(plan.iter().filter(|f| f.layer == layer && f.permit).all(|f| f.weight > block.weight));
        }
        assert_eq!(plan.iter().filter(|f| !f.permit).count(), 4);
        assert!(!plan.iter().any(|f| f.matches == WfpMatch::TunAdapter));
        assert!(plan.iter().any(|f| f.layer == WfpLayer::Connect4 && f.matches == WfpMatch::EngineTo(ep("203.0.113.7", 443))));
        assert!(plan.iter().any(|f| f.layer == WfpLayer::Connect6 && f.matches == WfpMatch::EngineTo(ep("2001:db8::7", 443))));
        // The engine may only connect out; nothing lets the endpoint rule apply on the accept layers.
        assert!(!plan.iter().any(|f| matches!(f.matches, WfpMatch::EngineTo(_)) && matches!(f.layer, WfpLayer::Accept4 | WfpLayer::Accept6)));
        let with_tun = wfp_plan(&[], true);
        assert_eq!(with_tun.iter().filter(|f| f.matches == WfpMatch::TunAdapter).count(), 4);
    }
}
