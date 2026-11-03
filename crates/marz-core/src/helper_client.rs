//! The app's side of the helper protocol. One short connection per request.
use marz_engine::ipc::{Request, Response, Status, MAX_LINE};
use marz_engine::EngineRequest;
use std::io::{BufRead, BufReader, Read, Write};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HelperError {
    /// The service is not installed or not running.
    Unreachable,
    Refused(String),
}

pub struct HelperClient {
    endpoint: String,
}

impl HelperClient {
    pub fn new(endpoint: impl Into<String>) -> Self {
        HelperClient { endpoint: endpoint.into() }
    }

    pub fn default_endpoint() -> &'static str {
        #[cfg(unix)]
        {
            marz_engine::ipc::LINUX_SOCKET
        }
        #[cfg(windows)]
        {
            marz_engine::ipc::WINDOWS_PIPE
        }
    }

    fn call(&self, request: &Request) -> Result<Response, HelperError> {
        let mut line = serde_json::to_vec(request).map_err(|_| HelperError::Refused("encode".into()))?;
        line.push(b'\n');
        let mut stream = connect(&self.endpoint).map_err(|_| HelperError::Unreachable)?;
        stream.write_all(&line).map_err(|_| HelperError::Unreachable)?;
        stream.flush().map_err(|_| HelperError::Unreachable)?;
        let mut reader = BufReader::new(stream.take(MAX_LINE as u64));
        let mut reply = Vec::new();
        reader.read_until(b'\n', &mut reply).map_err(|_| HelperError::Unreachable)?;
        match serde_json::from_slice::<Response>(&reply) {
            Ok(Response::Error { code }) => Err(HelperError::Refused(code)),
            Ok(r) => Ok(r),
            Err(_) => Err(HelperError::Unreachable),
        }
    }

    fn status_of(&self, request: &Request) -> Result<Status, HelperError> {
        match self.call(request)? {
            Response::Status(s) => Ok(s),
            _ => Err(HelperError::Refused("unexpected".into())),
        }
    }

    pub fn status(&self) -> Result<Status, HelperError> {
        self.status_of(&Request::Status)
    }

    pub fn start(&self, request: EngineRequest, kill_switch: bool) -> Result<Status, HelperError> {
        self.status_of(&Request::Start { request, kill_switch })
    }

    pub fn replace(&self, request: EngineRequest) -> Result<Status, HelperError> {
        self.status_of(&Request::Replace { request })
    }

    pub fn select(&self, tag: &str) -> Result<Status, HelperError> {
        self.status_of(&Request::Select { tag: tag.to_string() })
    }

    pub fn stop(&self) -> Result<Status, HelperError> {
        self.status_of(&Request::Stop)
    }

    pub fn emergency_off(&self) -> Result<Status, HelperError> {
        self.status_of(&Request::EmergencyOff)
    }

    pub fn test_route(&self, tag: &str) -> Result<Option<u32>, HelperError> {
        match self.call(&Request::TestRoute { tag: tag.to_string() })? {
            Response::Delay { ms } => Ok(ms),
            _ => Err(HelperError::Refused("unexpected".into())),
        }
    }
}

#[cfg(unix)]
fn connect(endpoint: &str) -> std::io::Result<std::os::unix::net::UnixStream> {
    let s = std::os::unix::net::UnixStream::connect(endpoint)?;
    s.set_read_timeout(Some(std::time::Duration::from_secs(15)))?;
    s.set_write_timeout(Some(std::time::Duration::from_secs(5)))?;
    Ok(s)
}

#[cfg(windows)]
fn connect(endpoint: &str) -> std::io::Result<std::fs::File> {
    // All pipe instances can be busy for a moment between two clients.
    let mut last = std::io::Error::other("pipe");
    for _ in 0..5 {
        match std::fs::OpenOptions::new().read(true).write(true).open(endpoint) {
            Ok(f) => return Ok(f),
            Err(e) if e.raw_os_error() == Some(231) => {
                last = e;
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Err(e) => return Err(e),
        }
    }
    Err(last)
}
