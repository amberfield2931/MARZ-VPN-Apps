//! A small HTTP/1.1 client over rustls. Own code because the session protocol needs a step between
//! "connection made" and "body sent" (RC6 records the operation durably there), and because it never
//! reads proxy settings: App API traffic must not go through a proxy configured by someone else.
use std::io::{self, ErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const READ_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_RESPONSE: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct Reply {
    pub code: u16,
    pub body: Vec<u8>,
}

#[derive(Clone)]
pub struct Endpoint {
    pub secure: bool,
    pub host: String,
    pub port: u16,
    tls: Option<Arc<rustls::ClientConfig>>,
}

impl Endpoint {
    /// Production: HTTPS only, the system's CAs plus the App API pins.
    pub fn parse(url: &str) -> Result<Self, &'static str> {
        let config = crate::tls::client_config().map_err(|_| "tls_config")?;
        Self::parse_with(url, config)
    }

    pub fn parse_with(url: &str, config: Arc<rustls::ClientConfig>) -> Result<Self, &'static str> {
        let (secure, rest) = if let Some(r) = url.strip_prefix("https://") {
            (true, r)
        } else if let (true, Some(r)) = (cfg!(feature = "lab"), url.strip_prefix("http://")) {
            (false, r)
        } else {
            return Err("https_required");
        };
        let authority = rest.trim_end_matches('/');
        if authority.is_empty() || authority.contains('/') || authority.contains('@') {
            return Err("invalid_url");
        }
        let (host, port) = match authority.rsplit_once(':') {
            Some((h, p)) => (h, p.parse().map_err(|_| "invalid_url")?),
            None => (authority, if secure { 443 } else { 80 }),
        };
        Ok(Endpoint { secure, host: host.to_string(), port, tls: secure.then_some(config) })
    }

    /// One request on a new connection. `before_body` runs when the connection is ready and nothing of
    /// the request has left yet; returning false aborts with an error.
    pub fn request(
        &self,
        method: &str,
        path: &str,
        headers: &[(&str, String)],
        body: &[u8],
        before_body: Option<&mut dyn FnMut() -> bool>,
    ) -> io::Result<Reply> {
        let tcp = self.connect()?;
        tcp.set_read_timeout(Some(READ_TIMEOUT))?;
        tcp.set_write_timeout(Some(READ_TIMEOUT))?;
        let mut head = format!("{method} {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Length: {}\r\n", self.authority(), body.len());
        for (k, v) in headers {
            if v.contains(['\r', '\n']) || k.contains(['\r', '\n']) {
                return Err(io::Error::new(ErrorKind::InvalidInput, "header"));
            }
            head.push_str(k);
            head.push_str(": ");
            head.push_str(v);
            head.push_str("\r\n");
        }
        head.push_str("\r\n");
        if let Some(config) = &self.tls {
            let name = rustls::pki_types::ServerName::try_from(self.host.clone()).map_err(|_| io::Error::new(ErrorKind::InvalidInput, "host"))?;
            let conn = rustls::ClientConnection::new(config.clone(), name).map_err(io::Error::other)?;
            let mut stream = rustls::StreamOwned::new(conn, tcp);
            // Finish the handshake first: a certificate failure must surface before anything is recorded or sent.
            while stream.conn.is_handshaking() {
                stream.conn.complete_io(&mut stream.sock)?;
            }
            exchange(&mut stream, &head, body, before_body)
        } else {
            let mut stream = tcp;
            exchange(&mut stream, &head, body, before_body)
        }
    }

    fn authority(&self) -> String {
        if self.port == if self.secure { 443 } else { 80 } {
            self.host.clone()
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    fn connect(&self) -> io::Result<TcpStream> {
        let mut last = io::Error::new(ErrorKind::NotFound, "no address");
        for addr in (self.host.as_str(), self.port).to_socket_addrs()? {
            match TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) {
                Ok(s) => return Ok(s),
                Err(e) => last = e,
            }
        }
        Err(last)
    }
}

fn exchange<S: Read + Write>(stream: &mut S, head: &str, body: &[u8], before_body: Option<&mut dyn FnMut() -> bool>) -> io::Result<Reply> {
    if let Some(hook) = before_body {
        if !hook() {
            return Err(io::Error::new(ErrorKind::Interrupted, "operation not saved"));
        }
    }
    let mut request = Vec::with_capacity(head.len() + body.len());
    request.extend_from_slice(head.as_bytes());
    request.extend_from_slice(body);
    stream.write_all(&request)?;
    stream.flush()?;
    read_reply(stream)
}

fn read_reply<S: Read>(stream: &mut S) -> io::Result<Reply> {
    let mut data = Vec::new();
    let mut buf = [0u8; 16 * 1024];
    let invalid = || io::Error::new(ErrorKind::InvalidData, "bad response");
    loop {
        let n = match stream.read(&mut buf) {
            Ok(n) => n,
            // A peer that closes without a TLS close_notify: the body's own length still decides below.
            Err(e) if e.kind() == ErrorKind::UnexpectedEof => 0,
            Err(e) => return Err(e),
        };
        data.extend_from_slice(&buf[..n]);
        if data.len() > MAX_RESPONSE {
            return Err(invalid());
        }
        let mut headers = [httparse::EMPTY_HEADER; 64];
        let mut parsed = httparse::Response::new(&mut headers);
        let status = parsed.parse(&data).map_err(|_| invalid())?;
        if let httparse::Status::Complete(start) = status {
            let code = parsed.code.ok_or_else(invalid)?;
            if (100..200).contains(&code) {
                data.drain(..start);
                continue;
            }
            let header = |name: &str| parsed.headers.iter().find(|h| h.name.eq_ignore_ascii_case(name)).map(|h| String::from_utf8_lossy(h.value).to_string());
            let chunked = header("transfer-encoding").is_some_and(|t| t.to_ascii_lowercase().contains("chunked"));
            let length: Option<usize> = header("content-length").and_then(|v| v.trim().parse().ok());
            if let Some(body) = complete_body(&data[start..], chunked, length, n == 0)? {
                return Ok(Reply { code, body });
            }
        } else if n == 0 {
            return Err(io::Error::new(ErrorKind::UnexpectedEof, "closed in headers"));
        }
        if n == 0 {
            return Err(io::Error::new(ErrorKind::UnexpectedEof, "closed in body"));
        }
    }
}

/// The whole body once it has arrived; None = keep reading. A cut-off body is an error, never a short answer.
fn complete_body(raw: &[u8], chunked: bool, length: Option<usize>, eof: bool) -> io::Result<Option<Vec<u8>>> {
    if chunked {
        return dechunk(raw);
    }
    match length {
        Some(n) if raw.len() >= n => Ok(Some(raw[..n].to_vec())),
        Some(_) => Ok(None),
        None if eof => Ok(Some(raw.to_vec())),
        None => Ok(None),
    }
}

fn dechunk(mut raw: &[u8]) -> io::Result<Option<Vec<u8>>> {
    let mut out = Vec::new();
    loop {
        let Some(line_end) = raw.windows(2).position(|w| w == b"\r\n") else { return Ok(None) };
        let size_text = std::str::from_utf8(&raw[..line_end]).map_err(|_| io::ErrorKind::InvalidData)?;
        let size = usize::from_str_radix(size_text.split(';').next().unwrap_or("").trim(), 16).map_err(|_| io::ErrorKind::InvalidData)?;
        raw = &raw[line_end + 2..];
        if size == 0 {
            return Ok(Some(out));
        }
        if raw.len() < size + 2 {
            return Ok(None);
        }
        out.extend_from_slice(&raw[..size]);
        raw = &raw[size + 2..];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(wire: &[u8]) -> io::Result<Reply> {
        read_reply(&mut &wire[..])
    }

    #[test]
    fn content_length_body() {
        let r = parse(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}").unwrap();
        assert_eq!((r.code, r.body.as_slice()), (200, b"{}".as_slice()));
    }

    #[test]
    fn chunked_body_and_interim_response() {
        let r = parse(b"HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 202 A\r\nTransfer-Encoding: chunked\r\n\r\n2;x=1\r\n{\"\r\n3\r\na\"}\r\n0\r\n\r\n").unwrap();
        assert_eq!(r.code, 202);
        assert_eq!(r.body, b"{\"a\"}");
    }

    #[test]
    fn body_until_close() {
        let r = parse(b"HTTP/1.1 500 X\r\n\r\nplain").unwrap();
        assert_eq!(r.body, b"plain");
    }

    #[test]
    fn truncated_answers_are_errors() {
        assert!(parse(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\n{}").is_err());
        assert!(parse(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nab").is_err());
        assert!(parse(b"HTTP/1.1 200").is_err());
        assert!(parse(b"garbage\r\n\r\n").is_err());
    }

    #[test]
    fn plain_http_is_refused_outside_lab_builds() {
        let config = crate::tls::client_config_with(rustls::RootCertStore::empty()).unwrap();
        let plain = Endpoint::parse_with("http://127.0.0.1:8787", config.clone());
        if cfg!(feature = "lab") {
            assert!(plain.is_ok());
        } else {
            assert_eq!(plain.err(), Some("https_required"));
        }
        assert_eq!(Endpoint::parse_with("https://u:p@host", config.clone()).err(), Some("invalid_url"));
        assert_eq!(Endpoint::parse_with("https://host/path", config.clone()).err(), Some("invalid_url"));
        assert_eq!(Endpoint::parse_with("https://app-api.marzvpn.com", config).unwrap().port, 443);
    }

    #[test]
    fn header_injection_is_refused() {
        let config = crate::tls::client_config_with(rustls::RootCertStore::empty()).unwrap();
        let e = Endpoint::parse_with("https://127.0.0.1:1", config).unwrap();
        // The connection fails first on a closed port; check the rule itself through a listener.
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let e2 = Endpoint { port: l.local_addr().unwrap().port(), ..e };
        let err = e2.request("GET", "/", &[("X", "a\r\nEvil: 1".into())], b"", None).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
    }
}
