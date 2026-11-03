//! Real traffic through the VPN, sent the way any app sends it (RC6 vpn/TunnelCheck): a plain socket that
//! the system routes into the tunnel. Nothing is added to the engine and the VPN is never touched.
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

const HOST: &str = "captive.apple.com";
const PATH: &str = "/hotspot-detect.html";
const SECOND_HOST: &str = "connectivitycheck.gstatic.com";
const SECOND_PATH: &str = "/generate_204";
const ROUNDS: usize = 3;
const TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckResult {
    /// The warm delay of the route in use, None when only the second site answered.
    Ok(Option<u32>),
    Failed { dns: bool },
}

pub fn run() -> CheckResult {
    run_against((HOST, PATH), (SECOND_HOST, SECOND_PATH))
}

pub fn run_against(first: (&str, &str), second: (&str, &str)) -> CheckResult {
    if let Some(ms) = warm(first.0, first.1) {
        return CheckResult::Ok(Some(ms));
    }
    if once(second.0, second.1) {
        return CheckResult::Ok(None);
    }
    CheckResult::Failed { dns: resolve(second.0).is_none() }
}

fn resolve(host: &str) -> Option<SocketAddr> {
    (host, 80).to_socket_addrs().ok()?.next()
}

fn connect(host: &str) -> Option<TcpStream> {
    let addr = resolve(host)?;
    let s = TcpStream::connect_timeout(&addr, TIMEOUT).ok()?;
    s.set_read_timeout(Some(TIMEOUT)).ok()?;
    s.set_write_timeout(Some(TIMEOUT)).ok()?;
    Some(s)
}

fn head(host: &str, path: &str, keep_alive: bool) -> String {
    format!("HEAD {path} HTTP/1.1\r\nHost: {host}\r\nConnection: {}\r\n\r\n", if keep_alive { "keep-alive" } else { "close" })
}

/// One request warms the path and is discarded; the lowest of the next rounds counts, two must succeed.
fn warm(host: &str, path: &str) -> Option<u32> {
    let mut s = connect(host)?;
    let request = head(host, path, true);
    exchange(&mut s, &request).ok()?;
    let times: Vec<u32> = (0..ROUNDS)
        .filter_map(|_| {
            let t = Instant::now();
            exchange(&mut s, &request).ok()?;
            Some(t.elapsed().as_millis() as u32)
        })
        .collect();
    if times.len() >= 2 {
        times.into_iter().min()
    } else {
        None
    }
}

fn once(host: &str, path: &str) -> bool {
    connect(host).is_some_and(|mut s| exchange(&mut s, &head(host, path, false)).is_ok())
}

/// Sends one HEAD request and reads its response headers (a HEAD response has no body).
fn exchange(s: &mut TcpStream, request: &str) -> std::io::Result<()> {
    s.write_all(request.as_bytes())?;
    let mut byte = [0u8; 1];
    let end = b"\r\n\r\n";
    let mut matched = 0;
    let mut first = true;
    let mut total = 0;
    while matched < 4 {
        s.read_exact(&mut byte)?;
        total += 1;
        if first {
            if byte[0] != b'H' {
                return Err(std::io::ErrorKind::InvalidData.into());
            }
            first = false;
        }
        if total > 64 * 1024 {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
        matched = if byte[0] == end[matched] { matched + 1 } else if byte[0] == end[0] { 1 } else { 0 };
    }
    Ok(())
}

/// Does the computer itself have a way out, apart from our tunnel? (RC6 NetworkIdentity.underlyingOnline)
/// The tunnel's routes live in their own table, so the main table still shows the real network.
pub fn underlying_network() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let text = std::fs::read_to_string("/proc/net/route").ok()?;
        for line in text.lines().skip(1) {
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.len() >= 4 && f[1] == "00000000" && f[0] != marz_engine::TUN_NAME && u32::from_str_radix(f[3], 16).unwrap_or(0) & 1 == 1 {
                return Some(format!("{}|{}", f[0], f[2]));
            }
        }
        let text = std::fs::read_to_string("/proc/net/ipv6_route").ok()?;
        for line in text.lines() {
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.len() >= 10 && f[0] == "00000000000000000000000000000000" && f[1] == "00" && f[9] != "lo" && f[9] != marz_engine::TUN_NAME {
                return Some(f[9].to_string());
            }
        }
        None
    }
    #[cfg(not(target_os = "linux"))]
    {
        Some("unknown".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// A loopback HTTP server: keeps a connection open and answers each HEAD.
    fn server(answer: &'static [u8]) -> String {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        std::thread::spawn(move || {
            for s in l.incoming().flatten() {
                std::thread::spawn(move || {
                    let mut s = s;
                    let mut buf = [0u8; 1024];
                    while let Ok(n) = s.read(&mut buf) {
                        if n == 0 || s.write_all(answer).is_err() {
                            break;
                        }
                    }
                });
            }
        });
        addr.to_string()
    }

    #[test]
    fn exchange_reads_headers_and_measures() {
        let addr = server(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
        let mut s = TcpStream::connect(&addr).unwrap();
        s.set_read_timeout(Some(TIMEOUT)).unwrap();
        for _ in 0..4 {
            exchange(&mut s, "HEAD / HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
        }
    }

    #[test]
    fn something_that_is_not_http_fails() {
        let addr = server(b"NOPE\r\n\r\n");
        let mut s = TcpStream::connect(&addr).unwrap();
        s.set_read_timeout(Some(TIMEOUT)).unwrap();
        assert!(exchange(&mut s, "HEAD / HTTP/1.1\r\n\r\n").is_err());
    }

    #[test]
    fn nothing_listening_is_a_failure_not_a_success() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        drop(l);
        assert!(!once("invalid.invalid", "/"));
    }
}
