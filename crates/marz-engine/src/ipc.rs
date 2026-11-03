//! App <-> helper protocol: one JSON object per line over a local socket (Linux) or pipe (Windows).
use crate::EngineRequest;
use serde::{Deserialize, Serialize};

pub const PROTOCOL: u32 = 4;
pub const MAX_LINE: usize = 256 * 1024;
pub const LINUX_SOCKET: &str = "/run/marzvpn/helper.sock";
pub const WINDOWS_PIPE: &str = r"\\.\pipe\marzvpn-helper";

fn protected() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Status,
    Start {
        request: EngineRequest,
        /// The user's setting. A request that does not say gets the block: fail closed.
        #[serde(default = "protected")]
        kill_switch: bool,
    },
    /// Apply new credentials or routes to a running tunnel (restarts the engine).
    Replace { request: EngineRequest },
    Select { tag: String },
    TestRoute { tag: String },
    Stop,
    /// Settings, after explicit confirmation, for when Stop did not bring the internet back. Answers only
    /// once the block is really gone.
    EmergencyOff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineState {
    Stopped,
    /// The engine is being started, or restarted after it exited, while the user's switch stays ON.
    Starting,
    Running,
    Stopping,
}

/// The independent firewall block. `Armed` while the VPN switch is ON with the Kill Switch setting on,
/// whatever the tunnel is doing. `Off` with the VPN on means the user turned the setting off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KillSwitch {
    Off,
    Armed,
}

/// Whose VPN session this is, from the point of view of the one asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Session {
    None,
    Mine,
    /// Another user of this computer turned the VPN on: they or an administrator can change it.
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Status {
    pub protocol: u32,
    pub state: EngineState,
    /// The user's switch. Stays true across engine exits and network losses; only Stop clears it.
    pub wanted: bool,
    pub selected: Option<String>,
    /// The route really in use, from the engine (not from the saved choice).
    pub active: Option<String>,
    /// Stable code of the last problem, never engine output.
    pub error: Option<String>,
    pub restarts: u32,
    pub kill_switch: KillSwitch,
    /// The switch is ON and blocking, but the helper has no routes (it restarted, or the computer did):
    /// only the app holds the credentials, and it should send Start.
    pub needs_request: bool,
    pub session: Session,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum Response {
    Status(Status),
    Delay { ms: Option<u32> },
    Error { code: String },
}
