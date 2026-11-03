use crate::{serve_client, Args};
use crate::supervisor::Shared;
use marz_engine::auth::Peer;
use marz_engine::ipc::LINUX_SOCKET;
use marz_engine::killswitch::{nft_dns_guard_remove_script, nft_dns_guard_script, nft_listing_intact, nft_remove_script, nft_script, Endpoint, NFT_TABLE};
use marz_engine::{RULE_INDEX, RULE_INDEX_END, ROUTE_TABLE, TUN_NAME};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

const DEFAULT_ENGINE: &str = "/usr/lib/marzvpn/sing-box";
const STATE_DIR: &str = "/var/lib/marzvpn";
const IP: [&str; 3] = ["/usr/sbin/ip", "/sbin/ip", "/usr/bin/ip"];
const NFT: [&str; 3] = ["/usr/sbin/nft", "/sbin/nft", "/usr/bin/nft"];
/// Real users only, and root: system services have no business here. Who may do what is decided per request.
const MIN_UID: u32 = 1000;

pub fn default_state_dir() -> PathBuf {
    PathBuf::from(STATE_DIR)
}

fn nft_path() -> Option<&'static str> {
    NFT.iter().copied().find(|p| std::path::Path::new(p).exists())
}

/// Runs one transaction from stdin. Absolute path, fixed arguments, no shell.
fn nft_run(script: &str) -> Result<(), &'static str> {
    use std::io::Write;
    let path = nft_path().ok_or("killswitch_no_firewall")?;
    let mut child = Command::new(path)
        .args(["-f", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "killswitch_failed")?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(script.as_bytes()).map_err(|_| "killswitch_failed")?;
    }
    let status = child.wait().map_err(|_| "killswitch_failed")?;
    if status.success() {
        Ok(())
    } else {
        Err("killswitch_failed")
    }
}

pub fn ks_apply(endpoints: &[Endpoint]) -> Result<(), &'static str> {
    nft_run(&nft_script(endpoints))
}

pub fn ks_remove() -> Result<(), &'static str> {
    nft_run(&nft_remove_script())
}

pub fn dns_guard_on() -> Result<(), &'static str> {
    nft_run(&nft_dns_guard_script())
}

pub fn ks_present() -> bool {
    let Some(path) = nft_path() else { return false };
    let Ok(out) = Command::new(path).args(["list", "table", "inet", NFT_TABLE]).stdin(Stdio::null()).stderr(Stdio::null()).output() else { return false };
    out.status.success() && nft_listing_intact(&String::from_utf8_lossy(&out.stdout))
}

/// Linux rules name the tunnel by interface name, so a new tunnel needs no change.
pub fn ks_needs_refresh() -> bool {
    false
}

pub fn main(args: Args) {
    let socket = args.endpoint.unwrap_or_else(|| LINUX_SOCKET.to_string());
    let engine = args.engine.unwrap_or_else(|| PathBuf::from(DEFAULT_ENGINE));
    let state_dir = args.state_dir.unwrap_or_else(default_state_dir);
    let shared = Shared::new(engine, &state_dir);
    for sig in [libc::SIGTERM, libc::SIGINT] {
        unsafe { libc::signal(sig, on_signal as *const () as libc::sighandler_t) };
    }
    {
        let s = shared.clone();
        std::thread::spawn(move || s.run());
    }
    {
        let s = shared.clone();
        std::thread::spawn(move || s.watchdog());
    }
    {
        let s = shared.clone();
        std::thread::spawn(move || {
            while !STOP.load(std::sync::atomic::Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(200));
            }
            // Service stop is not the user's OFF: restart, upgrade or shutdown keep the block. The engine
            // removes its own routes; anything left is cleaned here.
            s.shutdown();
            for _ in 0..60 {
                if s.engine_gone() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            cleanup_network();
            std::process::exit(0);
        });
    }
    let _ = std::fs::remove_file(&socket);
    if let Some(parent) = std::path::Path::new(&socket).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let listener = match UnixListener::bind(&socket) {
        Ok(l) => l,
        Err(_) => {
            eprintln!("marzvpn-helper: cannot bind the socket");
            std::process::exit(1);
        }
    };
    let _ = std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o666));
    for stream in listener.incoming().flatten() {
        let Some(peer) = peer_of(&stream) else { continue };
        let s = shared.clone();
        std::thread::spawn(move || serve_client(&s, stream, peer));
    }
}

static STOP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

extern "C" fn on_signal(_: libc::c_int) {
    STOP.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// The caller's identity from the kernel (SO_PEERCRED), fixed when it connected. None: not allowed to talk at all.
fn peer_of(stream: &UnixStream) -> Option<Peer> {
    let mut cred = libc::ucred { pid: 0, uid: u32::MAX, gid: 0 };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let rc = unsafe {
        libc::getsockopt(stream.as_raw_fd(), libc::SOL_SOCKET, libc::SO_PEERCRED, &mut cred as *mut _ as *mut libc::c_void, &mut len)
    };
    (rc == 0).then_some(cred.uid).and_then(peer_from_uid)
}

fn peer_from_uid(uid: u32) -> Option<Peer> {
    (uid == 0 || (uid >= MIN_UID && uid != 65534 && uid != u32::MAX)).then(|| Peer { id: format!("uid:{uid}"), admin: uid == 0 })
}

/// The engine dies with the helper, whatever kills the helper.
pub fn prepare_child(cmd: &mut Command) {
    unsafe {
        cmd.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        });
    }
}

pub fn after_spawn(_: &Child) {}

pub fn request_exit(child: &mut Child) {
    unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGTERM) };
}

fn ip_path() -> Option<&'static str> {
    IP.iter().copied().find(|p| std::path::Path::new(p).exists())
}

fn ip(args: &[&str]) -> Option<String> {
    let out = Command::new(ip_path()?).args(args).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The tunnel's interface is there and a policy rule still points at its table. IPv4 only: the config always
/// has an IPv4 address, IPv6 may be switched off on the computer, and a network manager removes both together.
pub fn tunnel_routes_intact() -> bool {
    if ip_path().is_some() && ip(&["link", "show", "dev", TUN_NAME]).is_none() {
        return false;
    }
    match ip(&["-4", "rule", "show"]) {
        Some(listing) => rules_reach_table(&listing, &format!("lookup {ROUTE_TABLE}")),
        // Cannot read them: no evidence of a loss.
        None => true,
    }
}

fn rules_reach_table(listing: &str, lookup: &str) -> bool {
    listing.is_empty() || listing.lines().any(|l| format!("{l} ").contains(&format!("{lookup} ")))
}

/// Priorities of the rules that are ours: inside our fixed range, or pointing at our table.
pub fn our_rule_priorities(listing: &str) -> Vec<u32> {
    let table = format!("lookup {ROUTE_TABLE}");
    let mut found = Vec::new();
    for line in listing.lines() {
        let Some(priority) = line.split(':').next().and_then(|p| p.trim().parse::<u32>().ok()) else { continue };
        let padded = format!("{line} ");
        if (RULE_INDEX..=RULE_INDEX_END).contains(&priority) || padded.contains(&format!("{table} ")) {
            found.push(priority);
        }
    }
    found
}

/// After any engine exit, also SIGKILL or power-cut style ones: removes our leftover policy rules, routes
/// and interface. Touches nothing outside our fixed priority range, table and interface name, and not the
/// kill switch table: with the switch ON the traffic these routes carried is now refused by the firewall.
pub fn cleanup_network() {
    let _ = nft_run(&nft_dns_guard_remove_script());
    let Some(_) = ip_path() else { return };
    for family in ["-4", "-6"] {
        if let Some(listing) = ip(&[family, "rule", "show"]) {
            for priority in our_rule_priorities(&listing) {
                let _ = ip(&[family, "rule", "del", "priority", &priority.to_string()]);
            }
        }
        let _ = ip(&[family, "route", "flush", "table", &ROUTE_TABLE.to_string()]);
    }
    if ip(&["link", "show", "dev", TUN_NAME]).is_some() {
        let _ = ip(&["link", "del", TUN_NAME]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_root_and_real_users_may_connect_and_only_root_is_admin() {
        assert_eq!(peer_from_uid(0), Some(Peer { id: "uid:0".into(), admin: true }));
        assert_eq!(peer_from_uid(1000), Some(Peer { id: "uid:1000".into(), admin: false }));
        for uid in [1, 33, 999, 65534, u32::MAX] {
            assert_eq!(peer_from_uid(uid), None, "{uid}");
        }
    }

    #[test]
    fn lost_tunnel_rules_are_noticed() {
        let lookup = "lookup 2022";
        assert!(rules_reach_table("0:\tfrom all lookup local\n9000:\tfrom all to 172.19.0.0/30 lookup 2022\n", lookup));
        assert!(!rules_reach_table("0:\tfrom all lookup local\n9002:\tfrom all iif marzvpn0 goto 9010 [unresolved]\n32766:\tfrom all lookup main\n", lookup));
        assert!(!rules_reach_table("9001:\tfrom all lookup 20220\n", lookup));
    }

    #[test]
    fn only_our_rules_are_selected() {
        let listing = "0:\tfrom all lookup local\n\
            5210:\tfrom all fwmark 0x80000/0xff0000 lookup main\n\
            5270:\tfrom all lookup 52\n\
            9000:\tfrom all to 172.19.0.0/30 lookup 2022\n\
            9001:\tfrom all iif marzvpn0 goto 9003\n\
            9010:\tnot from all goto 32766\n\
            9011:\tfrom all lookup 20220\n\
            8999:\tfrom 10.0.0.1 lookup 2022 \n\
            32766:\tfrom all lookup main\n\
            32767:\tfrom all lookup default\n";
        assert_eq!(our_rule_priorities(listing), vec![9000, 9001, 9010, 8999]);
    }
}
