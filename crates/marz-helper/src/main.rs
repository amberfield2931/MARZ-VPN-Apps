//! marzvpn-helper: the only privileged part of MARZ VPN. Runs as a system service, accepts typed
//! requests from the unprivileged app over a local socket, validates them and supervises one
//! sing-box process. Nothing from a request reaches a shell or a file; see marz-engine for the shape.
mod killswitch;
mod platform;
mod supervisor;

use marz_engine::auth::Peer;
use marz_engine::ipc::{Request, Response, MAX_LINE};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::sync::Arc;
use supervisor::Shared;

pub struct Args {
    pub endpoint: Option<String>,
    pub engine: Option<PathBuf>,
    pub state_dir: Option<PathBuf>,
    pub action: Action,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Serve,
    /// Early boot, before the network: put the block back if the switch was left ON. Then exit.
    ArmFromState,
    /// By hand or on package removal: remove what MARZ VPN holds in the network, and only that.
    Recover,
}

fn parse_args() -> Args {
    let mut args = Args { endpoint: None, engine: None, state_dir: None, action: Action::Serve };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--socket" => args.endpoint = it.next(),
            "--engine" => args.engine = it.next().map(PathBuf::from),
            "--state-dir" => args.state_dir = it.next().map(PathBuf::from),
            "--arm-from-state" => args.action = Action::ArmFromState,
            "--recover" => args.action = Action::Recover,
            "--service" => {}
            "--version" => {
                println!("marzvpn-helper {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            _ => {
                eprintln!("usage: marzvpn-helper [--socket PATH] [--engine PATH] [--state-dir PATH] [--arm-from-state | --recover]");
                std::process::exit(2);
            }
        }
    }
    args
}

/// One request per line; a line over the limit ends the connection before it is parsed.
pub fn serve_client<S: Read + Write>(shared: &Arc<Shared>, stream: S, peer: Peer) {
    let mut reader = BufReader::new(stream);
    loop {
        let mut line = Vec::new();
        let n = match (&mut reader).take(MAX_LINE as u64 + 1).read_until(b'\n', &mut line) {
            Ok(n) => n,
            Err(_) => return,
        };
        if n == 0 {
            return;
        }
        if line.len() > MAX_LINE {
            let _ = reply(reader.get_mut(), &Response::Error { code: "too_large".into() });
            return;
        }
        let response = match serde_json::from_slice::<Request>(&line) {
            Ok(request) => handle(shared, request, &peer),
            Err(_) => Response::Error { code: "bad_request".into() },
        };
        if reply(reader.get_mut(), &response).is_err() {
            return;
        }
    }
}

fn reply<S: Write>(stream: &mut S, response: &Response) -> std::io::Result<()> {
    let mut text = serde_json::to_vec(response).map_err(std::io::Error::other)?;
    text.push(b'\n');
    stream.write_all(&text)?;
    stream.flush()
}

/// Every change goes through a Shared method that checks `peer` against the session's owner.
fn handle(shared: &Arc<Shared>, request: Request, peer: &Peer) -> Response {
    let error = |code: &str| Response::Error { code: code.into() };
    let status = || Response::Status(shared.status_for(peer));
    match request {
        Request::Status => status(),
        Request::Start { request, kill_switch } => match request.validate() {
            Ok(()) => match shared.start(peer, request, Some(kill_switch)) {
                Ok(()) => status(),
                Err(code) => error(code),
            },
            Err(e) => error(e.0),
        },
        Request::Replace { request } => match request.validate() {
            // New routes keep the mode the tunnel was started in.
            Ok(()) if shared.wanted() => match shared.start(peer, request, None) {
                Ok(()) => status(),
                Err(code) => error(code),
            },
            Ok(()) => error("not_running"),
            Err(e) => error(e.0),
        },
        Request::Select { tag } => match shared.select(peer, &tag) {
            Ok(()) => status(),
            Err(code) => error(code),
        },
        Request::TestRoute { tag } => match shared.test_route(peer, &tag) {
            Ok(ms) => Response::Delay { ms },
            Err(code) => error(code),
        },
        Request::Stop => match shared.stop(peer) {
            Ok(()) => status(),
            Err(code) => error(code),
        },
        Request::EmergencyOff => match shared.emergency_off(peer) {
            Ok(()) => status(),
            Err(code) => error(code),
        },
    }
}

fn main() {
    let args = parse_args();
    let state_dir = args.state_dir.clone().unwrap_or_else(platform::default_state_dir);
    match args.action {
        Action::Serve => platform::main(args),
        Action::ArmFromState => killswitch::arm_from_state(&state_dir),
        Action::Recover => match killswitch::recover(&state_dir) {
            Ok(()) => println!("MARZ VPN no longer blocks the network."),
            Err(code) => {
                eprintln!("marzvpn-helper: could not remove the block ({code}). Are you root?");
                std::process::exit(1);
            }
        },
    }
}
