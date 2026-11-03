//! Keeps the engine running while the user's switch is ON. The switch never changes by itself: an
//! engine exit or a network loss means "restart with a backoff", and only Stop turns it off.
use crate::killswitch::KillSwitch;
use marz_engine::auth::{may_control, may_see, Peer, NOT_OWNER};
use marz_engine::ipc::{EngineState, KillSwitch as KillSwitchState, Session, Status, PROTOCOL};
use marz_engine::killswitch::{endpoints_of, pin_routes};
use marz_engine::{build_config, Controller, EngineRequest, AUTO_TAG, SELECTOR_TAG};
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

const MAX_BACKOFF_SECS: u64 = 30;
/// An engine that stayed up this long was healthy: the next exit starts the backoff from the beginning.
const HEALTHY_SECS: u64 = 60;
const READY_TIMEOUT: Duration = Duration::from_secs(20);
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(4);
const WATCH_EVERY: Duration = Duration::from_secs(1);

pub struct Shared {
    inner: Mutex<Inner>,
    wake: Condvar,
    engine: PathBuf,
    ks: KillSwitch,
    /// One firewall change at a time, and never between "decide" and "apply". Taken before `inner`.
    ks_op: Mutex<()>,
}

struct Inner {
    wanted: Option<EngineRequest>,
    generation: u64,
    state: EngineState,
    active: Option<String>,
    error: Option<String>,
    restarts: u32,
    controller: Option<Controller>,
    /// The user turned the switch off: when the engine is gone, the block comes down.
    disarm: bool,
    /// The service is stopping (restart, upgrade, shutdown): the engine goes, the block stays.
    shutting_down: bool,
    engine_alive: bool,
    /// Who turned the VPN on. Survives a restart through the saved state when the block is up.
    owner: Option<String>,
}

impl Shared {
    pub fn new(engine: PathBuf, state_dir: &std::path::Path) -> Arc<Self> {
        let ks = KillSwitch::new(state_dir);
        if ks.saved() && ks.resume().is_err() {
            eprintln!("marzvpn-helper: could not restore the kill switch");
        }
        let owner = if ks.saved() { ks.owner() } else { None };
        Arc::new(Shared {
            inner: Mutex::new(Inner {
                owner,
                wanted: None,
                generation: 0,
                state: EngineState::Stopped,
                active: None,
                error: None,
                restarts: 0,
                controller: None,
                disarm: false,
                shutting_down: false,
                engine_alive: false,
            }),
            wake: Condvar::new(),
            engine,
            ks,
            ks_op: Mutex::new(()),
        })
    }

    fn active(&self, g: &Inner) -> bool {
        g.wanted.is_some() || self.ks.is_armed() || self.ks.saved()
    }

    /// Err when this peer may not change the current session.
    fn authorize(&self, peer: &Peer) -> Result<(), &'static str> {
        let g = self.inner.lock().unwrap();
        if may_control(peer, self.active(&g), g.owner.as_deref()) {
            Ok(())
        } else {
            Err(NOT_OWNER)
        }
    }

    /// The service's own view: internal waits and an administrator's answers.
    pub fn status(&self) -> Status {
        let g = self.inner.lock().unwrap();
        let armed = self.ks.is_armed() || self.ks.saved();
        Status {
            protocol: PROTOCOL,
            // Stopped means the block is gone too: until then the internet is not back.
            state: if armed && g.state == EngineState::Stopped { EngineState::Starting } else { g.state },
            wanted: g.wanted.is_some() || armed,
            selected: g.wanted.as_ref().map(|r| r.selected.clone()),
            active: g.active.clone(),
            error: g.error.clone(),
            restarts: g.restarts,
            kill_switch: if armed { KillSwitchState::Armed } else { KillSwitchState::Off },
            // After a failed OFF the app must not bring the tunnel back: the user's next press retries the OFF.
            needs_request: armed && g.wanted.is_none() && !g.disarm && g.error.as_deref() != Some("killswitch_remove_failed"),
            session: if g.wanted.is_some() || armed { Session::Other } else { Session::None },
        }
    }

    /// What one peer is told. Someone else's session shows only that it exists and what it does to the
    /// network: not the routes, errors or a request for credentials.
    pub fn status_for(&self, peer: &Peer) -> Status {
        let mut s = self.status();
        let owner = self.inner.lock().unwrap().owner.clone();
        if s.session == Session::None {
            return s;
        }
        if owner.as_deref() == Some(peer.id.as_str()) {
            s.session = Session::Mine;
        } else if !may_see(peer, owner.as_deref()) {
            s.selected = None;
            s.active = None;
            s.error = None;
            s.restarts = 0;
        }
        if owner.as_deref() != Some(peer.id.as_str()) {
            // Only the owner holds the credentials this asks for; an administrator resumes nothing by accident.
            s.needs_request = false;
        }
        s
    }

    /// New desired tunnel. With the Kill Switch setting on, the block goes up (or changes, atomically) before
    /// the engine exists; if that fails no engine starts. Err leaves everything as it was, including any block
    /// already in place. `kill_switch` None keeps the current mode. A block already up always stays: Start can
    /// never lower it, only the user's OFF can.
    pub fn start(&self, peer: &Peer, request: EngineRequest, kill_switch: Option<bool>) -> Result<(), &'static str> {
        self.authorize(peer)?;
        let tunnel_up = self.inner.lock().unwrap().state == EngineState::Running;
        // Without the block a name is resolved like any other program's query; with it, see below.
        let armed = self.ks.is_armed() || self.ks.saved();
        let mut names = Vec::new();
        let pinned = pin_routes(&request, |name| {
            // With the block up, a query can only go through a working tunnel; without one the address saved
            // when the name last resolved is all there is. Off, this is ordinary traffic of an unprotected
            // computer, and it happens before anything is blocked.
            let ip = if !armed || tunnel_up { resolve(name) } else { None }.or_else(|| self.ks.pin(name))?;
            names.push((name.to_string(), ip));
            Some(ip)
        })
        .map_err(|e| e.0)?;
        let endpoints = endpoints_of(&pinned).map_err(|e| e.0)?;
        let _op = self.ks_op.lock().unwrap();
        // Again under the lock: the session may have changed hands during the name lookups above.
        self.authorize(peer)?;
        let armed_now = self.ks.is_armed() || self.ks.saved();
        // A session keeps its owner whoever changes it. An administrator takes one over only if it has none.
        let owner = {
            let g = self.inner.lock().unwrap();
            if self.active(&g) { g.owner.clone().unwrap_or_else(|| peer.id.clone()) } else { peer.id.clone() }
        };
        if armed_now || kill_switch.unwrap_or(false) {
            self.ks.arm(&endpoints, &names, Some(&owner))?;
        }
        let mut g = self.inner.lock().unwrap();
        g.owner = Some(owner);
        g.wanted = Some(pinned);
        g.generation += 1;
        g.error = None;
        g.restarts = 0;
        g.disarm = false;
        self.wake.notify_all();
        Ok(())
    }

    /// The owner's or an administrator's explicit OFF. The only request that removes the block.
    pub fn stop(&self, peer: &Peer) -> Result<(), &'static str> {
        let mut g = self.inner.lock().unwrap();
        if !may_control(peer, self.active(&g), g.owner.as_deref()) {
            return Err(NOT_OWNER);
        }
        g.wanted = None;
        g.generation += 1;
        g.error = None;
        g.disarm = true;
        self.wake.notify_all();
        Ok(())
    }

    /// The way out when the ordinary OFF did not work: does not depend on the supervisor loop finishing.
    /// Err means the block is still up, and says so.
    pub fn emergency_off(&self, peer: &Peer) -> Result<(), &'static str> {
        self.stop(peer)?;
        for _ in 0..80 {
            if self.status().state == EngineState::Stopped {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _op = self.ks_op.lock().unwrap();
        crate::platform::cleanup_network();
        self.ks.disarm()?;
        let mut g = self.inner.lock().unwrap();
        g.disarm = false;
        g.error = None;
        g.owner = None;
        Ok(())
    }

    /// Service stop: restart, upgrade or shutdown. The engine stops; the block and the saved state stay.
    pub fn shutdown(&self) {
        let mut g = self.inner.lock().unwrap();
        g.wanted = None;
        g.generation += 1;
        g.shutting_down = true;
        self.wake.notify_all();
    }

    pub fn engine_gone(&self) -> bool {
        !self.inner.lock().unwrap().engine_alive
    }

    pub fn wanted(&self) -> bool {
        self.inner.lock().unwrap().wanted.is_some() || self.ks.is_armed()
    }

    /// The block is a kernel table; this keeps it there if a firewall reload or a person removes it while
    /// the switch is ON, and notices the manual recovery.
    pub fn watchdog(self: Arc<Self>) -> ! {
        loop {
            std::thread::sleep(WATCH_EVERY);
            let _op = self.ks_op.lock().unwrap();
            if self.ks.removed_outside() {
                // The manual recovery may have raced this loop's last check of the table.
                let _ = crate::platform::ks_remove();
                self.ks.forget();
                let mut g = self.inner.lock().unwrap();
                g.wanted = None;
                g.generation += 1;
                g.error = None;
                g.owner = None;
                self.wake.notify_all();
            } else if let Some(code) = self.ks.watch() {
                self.inner.lock().unwrap().error = Some(code.to_string());
            }
        }
    }

    /// Everything that happens with the switch OFF and no engine: takes the block down when asked to.
    fn finish_stop(&self) {
        let _op = self.ks_op.lock().unwrap();
        let mut g = self.inner.lock().unwrap();
        if g.wanted.is_some() || !g.disarm {
            return;
        }
        g.disarm = false;
        if (self.ks.is_armed() || self.ks.saved()) && self.ks.disarm().is_err() {
            // Still blocking and still ON: pressing OFF again retries.
            g.error = Some("killswitch_remove_failed".to_string());
            return;
        }
        g.owner = None;
    }

    /// Switch route in the running engine (no restart, like RC6). The saved request also changes, so a
    /// later engine restart keeps the choice.
    pub fn select(&self, peer: &Peer, tag: &str) -> Result<(), &'static str> {
        self.authorize(peer)?;
        let (controller, known) = {
            let g = self.inner.lock().unwrap();
            let Some(req) = g.wanted.as_ref() else { return Err("not_running") };
            let known = tag == AUTO_TAG || req.routes.iter().any(|r| r.tag == tag);
            (g.controller.clone(), known)
        };
        if !known {
            return Err("unknown_route");
        }
        if let Some(c) = controller {
            let body = format!("{{\"name\":\"{tag}\"}}"); // tag checked against the validated list above
            let (code, _) = api(&c, "PUT", &format!("/proxies/{SELECTOR_TAG}"), Some(&body)).map_err(|_| "engine_unreachable")?;
            if code / 100 != 2 {
                return Err("engine_refused");
            }
        }
        let mut g = self.inner.lock().unwrap();
        if let Some(req) = g.wanted.as_mut() {
            req.selected = tag.to_string();
        }
        Ok(())
    }

    /// A cold check of one route through the running engine. None = it failed.
    pub fn test_route(&self, peer: &Peer, tag: &str) -> Result<Option<u32>, &'static str> {
        self.authorize(peer)?;
        let controller = {
            let g = self.inner.lock().unwrap();
            let Some(req) = g.wanted.as_ref() else { return Err("not_running") };
            if tag != AUTO_TAG && !req.routes.iter().any(|r| r.tag == tag) {
                return Err("unknown_route");
            }
            g.controller.clone()
        };
        let c = controller.ok_or("not_running")?;
        // No url parameter: the engine refuses plain-HTTP test URLs here and uses its own HTTPS default.
        let path = format!("/proxies/{tag}/delay?timeout=5000");
        match api(&c, "GET", &path, None) {
            Ok((200, body)) => Ok(serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| v.get("delay").and_then(|d| d.as_u64()))
                .map(|d| d as u32)),
            Ok(_) => Ok(None),
            Err(_) => Err("engine_unreachable"),
        }
    }

    pub fn run(self: Arc<Self>) -> ! {
        crate::platform::cleanup_network();
        loop {
            let (request, generation) = {
                let mut g = self.inner.lock().unwrap();
                while g.wanted.is_none() {
                    g.active = None;
                    g.controller = None;
                    g.engine_alive = false;
                    if g.disarm {
                        g.state = EngineState::Stopping;
                        drop(g);
                        self.finish_stop();
                        g = self.inner.lock().unwrap();
                        continue;
                    }
                    g.state = EngineState::Stopped;
                    g = self.wake.wait(g).unwrap();
                }
                if g.shutting_down {
                    g.wanted = None;
                    continue;
                }
                g.engine_alive = true;
                g.state = EngineState::Starting;
                (g.wanted.clone().unwrap(), g.generation)
            };
            let started = Instant::now();
            let outcome = self.run_engine(&request, generation);
            // Routes and the interface only. The block is separate and stays until an explicit OFF.
            crate::platform::cleanup_network();
            let mut g = self.inner.lock().unwrap();
            g.active = None;
            g.controller = None;
            if g.generation != generation {
                continue;
            }
            if started.elapsed().as_secs() >= HEALTHY_SECS {
                g.restarts = 0;
            }
            g.restarts = g.restarts.saturating_add(1);
            g.error = Some(outcome.to_string());
            g.state = EngineState::Starting;
            let wait = Duration::from_secs((1u64 << g.restarts.min(5)).min(MAX_BACKOFF_SECS));
            let deadline = Instant::now() + wait;
            while g.generation == generation {
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    break;
                }
                g = self.wake.wait_timeout(g, left).unwrap().0;
            }
        }
    }

    /// Runs one engine process until it exits or this generation ends. Returns why it ended.
    fn run_engine(&self, request: &EngineRequest, generation: u64) -> &'static str {
        // With the block up DNS is already refused outside the tunnel. Without it, the guard comes first; if it
        // cannot be set no engine starts, because DNS through the tunnel could not be promised.
        if !(self.ks.is_armed() || self.ks.saved()) && crate::platform::dns_guard_on().is_err() {
            return "killswitch_failed";
        }
        let Ok(controller) = new_controller() else { return "engine_controller" };
        let config = build_config(request, &controller).to_string();
        let mut child = match spawn(&self.engine, &config) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return "engine_missing",
            Err(_) => return "engine_spawn",
        };
        let stderr = child.stderr.take();
        let reader = stderr.map(|mut s| {
            std::thread::spawn(move || {
                let mut text = Vec::new();
                let _ = (&mut s).take(64 * 1024).read_to_end(&mut text);
                let mut sink = [0u8; 4096];
                while matches!(s.read(&mut sink), Ok(n) if n > 0) {}
                classify(&String::from_utf8_lossy(&text))
            })
        });
        {
            let mut g = self.inner.lock().unwrap();
            if g.generation == generation {
                g.controller = Some(controller.clone());
            }
        }
        let begun = Instant::now();
        let mut ready = false;
        let mut last_poll = Instant::now() - Duration::from_secs(10);
        loop {
            if self.inner.lock().unwrap().generation != generation {
                let mut g = self.inner.lock().unwrap();
                g.state = EngineState::Stopping;
                drop(g);
                stop_child(&mut child);
                return "stopped";
            }
            match child.try_wait() {
                Ok(Some(_)) | Err(_) => {
                    let why = reader.and_then(|r| r.join().ok()).unwrap_or("engine_exit");
                    return why;
                }
                Ok(None) => {}
            }
            if last_poll.elapsed() >= Duration::from_secs(if ready { 3 } else { 1 }) {
                last_poll = Instant::now();
                // A network manager can remove the tunnel's routing rules when the network changes (systemd-networkd
                // does). Traffic is then refused by the block, safely, but it would stay that way: start over.
                if ready && !crate::platform::tunnel_routes_intact() {
                    stop_child(&mut child);
                    return "routes_lost";
                }
                match active_route(&controller) {
                    Some(active) => {
                        let mut g = self.inner.lock().unwrap();
                        if g.generation == generation {
                            if !ready {
                                ready = true;
                                g.state = EngineState::Running;
                                g.error = None;
                            }
                            g.active = Some(active);
                        }
                    }
                    None if !ready && begun.elapsed() > READY_TIMEOUT => {
                        stop_child(&mut child);
                        return "engine_timeout";
                    }
                    None => {}
                }
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}

fn spawn(engine: &PathBuf, config: &str) -> std::io::Result<Child> {
    let mut cmd = Command::new(engine);
    // The config goes over stdin: no credentials on disk or in the process list.
    cmd.args(["run", "-c", "stdin", "--disable-color"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    crate::platform::prepare_child(&mut cmd);
    let mut child = cmd.spawn()?;
    crate::platform::after_spawn(&child);
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(config.as_bytes());
    }
    Ok(child)
}

fn stop_child(child: &mut Child) {
    crate::platform::request_exit(child);
    for _ in 0..50 {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Engine output stays in memory and is reduced to a stable code: it can name servers and addresses.
fn classify(text: &str) -> &'static str {
    let t = text.to_ascii_lowercase();
    if t.contains("operation not permitted") || t.contains("permission denied") {
        "tun_permission"
    } else if t.contains("decode config") || t.contains("create service") || t.contains("initialize") {
        "engine_config"
    } else {
        "engine_exit"
    }
}

/// The system resolver, bounded: a query must not hold up the app's request for long.
fn resolve(name: &str) -> Option<IpAddr> {
    let (tx, rx) = std::sync::mpsc::channel();
    let name = name.to_string();
    std::thread::spawn(move || {
        let found: Vec<IpAddr> = (name.as_str(), 443u16).to_socket_addrs().map(|a| a.map(|s| s.ip()).collect()).unwrap_or_default();
        let _ = tx.send(found);
    });
    let found = rx.recv_timeout(RESOLVE_TIMEOUT).ok()?;
    let usable = |ip: &&IpAddr| marz_engine::killswitch::endpoint_address_allowed(**ip);
    found.iter().filter(usable).find(|ip| ip.is_ipv4()).or_else(|| found.iter().find(usable)).copied()
}

fn random_hex(bytes: usize) -> Result<String, ()> {
    let mut b = vec![0u8; bytes];
    getrandom::fill(&mut b).map_err(|_| ())?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}

fn new_controller() -> Result<Controller, ()> {
    let l = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|_| ())?;
    let port = l.local_addr().map_err(|_| ())?.port();
    drop(l);
    Ok(Controller { port, secret: random_hex(32)? })
}

fn active_route(c: &Controller) -> Option<String> {
    let member = |name: &str| -> Option<String> {
        let (code, body) = api(c, "GET", &format!("/proxies/{name}"), None).ok()?;
        if code != 200 {
            return None;
        }
        let v: serde_json::Value = serde_json::from_str(&body).ok()?;
        v.get("now")?.as_str().map(str::to_string)
    };
    let selected = member(SELECTOR_TAG)?;
    if selected == AUTO_TAG {
        Some(member(AUTO_TAG).unwrap_or(selected))
    } else {
        Some(selected)
    }
}

/// Minimal HTTP/1.0 client for the engine's own loopback controller (fixed host, secret in a header).
fn api(c: &Controller, method: &str, path: &str, body: Option<&str>) -> std::io::Result<(u32, String)> {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, c.port));
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_secs(2))?;
    s.set_read_timeout(Some(Duration::from_secs(8)))?;
    s.set_write_timeout(Some(Duration::from_secs(2)))?;
    let body = body.unwrap_or("");
    write!(
        s,
        "{method} {path} HTTP/1.0\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        c.secret,
        body.len()
    )?;
    let mut text = Vec::new();
    s.take(1024 * 1024).read_to_end(&mut text)?;
    let text = String::from_utf8_lossy(&text);
    let (head, rest) = text.split_once("\r\n\r\n").ok_or(std::io::ErrorKind::InvalidData)?;
    let code = head.split_whitespace().nth(1).and_then(|c| c.parse().ok()).ok_or(std::io::ErrorKind::InvalidData)?;
    Ok((code, rest.to_string()))
}
