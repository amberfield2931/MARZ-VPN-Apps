//! The part of the kill switch every platform shares: the saved "switch is ON" fact and the order of
//! operations. Saved state first, firewall second: a crash between the two errs on the side of blocking.
use marz_engine::killswitch::{ArmedState, Endpoint, ARMED_VERSION};
use std::io::Write;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const FILE: &str = "killswitch.json";

pub struct KillSwitch {
    path: PathBuf,
    /// What is applied now, so the watchdog can put back exactly this.
    current: Mutex<Option<Vec<Endpoint>>>,
    pins: Mutex<ArmedState>,
}

impl KillSwitch {
    pub fn new(state_dir: &Path) -> Self {
        let path = state_dir.join(FILE);
        let pins = load(&path).unwrap_or_default();
        KillSwitch { path, current: Mutex::new(None), pins: Mutex::new(pins) }
    }

    /// The switch was left ON (by a helper that died, a restart, a reboot).
    pub fn saved(&self) -> bool {
        self.path.exists()
    }

    pub fn is_armed(&self) -> bool {
        self.current.lock().unwrap().is_some()
    }

    pub fn owner(&self) -> Option<String> {
        self.pins.lock().unwrap().owner.clone()
    }

    pub fn pin(&self, name: &str) -> Option<IpAddr> {
        self.pins.lock().unwrap().pins.get(name).copied()
    }

    /// Replaces the block atomically. Err leaves the previous block in place, if there was one.
    pub fn arm(&self, endpoints: &[Endpoint], names: &[(String, IpAddr)], owner: Option<&str>) -> Result<(), &'static str> {
        let mut state = self.pins.lock().unwrap().clone();
        state.version = ARMED_VERSION;
        state.owner = owner.map(str::to_string);
        for (n, ip) in names {
            state.pins.insert(n.clone(), *ip);
        }
        let was_saved = self.saved();
        save(&self.path, &state).map_err(|_| "killswitch_state")?;
        if let Err(code) = crate::platform::ks_apply(endpoints).and_then(|_| if crate::platform::ks_present() { Ok(()) } else { Err("killswitch_failed") }) {
            if !was_saved && self.current.lock().unwrap().is_none() {
                let _ = std::fs::remove_file(&self.path);
            }
            return Err(code);
        }
        *self.pins.lock().unwrap() = state;
        *self.current.lock().unwrap() = Some(endpoints.to_vec());
        Ok(())
    }

    /// Restores the block after a restart: no endpoints until the app gives routes again.
    pub fn resume(&self) -> Result<(), &'static str> {
        crate::platform::ks_apply(&[])?;
        *self.current.lock().unwrap() = Some(Vec::new());
        Ok(())
    }

    /// Firewall first, saved state second: if the removal fails the switch stays ON and blocking.
    pub fn disarm(&self) -> Result<(), &'static str> {
        crate::platform::ks_remove()?;
        let _ = std::fs::remove_file(&self.path);
        sync_dir(&self.path);
        *self.current.lock().unwrap() = None;
        *self.pins.lock().unwrap() = ArmedState::default();
        Ok(())
    }

    /// Someone removed the saved state by hand (`--recover`): that is an explicit OFF.
    pub fn removed_outside(&self) -> bool {
        self.is_armed() && !self.saved()
    }

    pub fn forget(&self) {
        *self.current.lock().unwrap() = None;
        *self.pins.lock().unwrap() = ArmedState::default();
    }

    /// Puts the block back if anything took it away. None: all is well. Otherwise the code to report.
    pub fn watch(&self) -> Option<&'static str> {
        let endpoints = self.current.lock().unwrap().clone()?;
        if crate::platform::ks_needs_refresh() {
            // Windows: the filters permit the tunnel adapter by its identity, known once the engine made it.
            return crate::platform::ks_apply(&endpoints).err().map(|_| "killswitch_lost");
        }
        if crate::platform::ks_present() {
            return None;
        }
        match crate::platform::ks_apply(&endpoints) {
            Ok(()) => Some("killswitch_restored"),
            Err(_) => Some("killswitch_lost"),
        }
    }
}

fn load(path: &Path) -> Option<ArmedState> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// Temporary file, flush, rename, flush the directory: after a power cut the file is whole or absent.
fn save(path: &Path, state: &ArmedState) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(&serde_json::to_vec(state).map_err(std::io::Error::other)?)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    sync_dir(path);
    Ok(())
}

fn sync_dir(path: &Path) {
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        if let Ok(d) = std::fs::File::open(parent) {
            let _ = d.sync_all();
        }
    }
    #[cfg(not(unix))]
    let _ = path;
}

/// `--recover`: the documented way back to normal internet, independent of the running service.
pub fn recover(state_dir: &Path) -> Result<(), &'static str> {
    // State first. While it exists a running service treats a missing table as tampering and puts it back, and that
    // would leave the computer blocked with the switch reported OFF. Without rights nothing else is attempted.
    let path = state_dir.join(FILE);
    match std::fs::remove_file(&path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err("killswitch_failed"),
        _ => {}
    }
    sync_dir(&path);
    crate::platform::cleanup_network();
    crate::platform::ks_remove()
}

/// `--arm-from-state`: early boot, before the network comes up.
pub fn arm_from_state(state_dir: &Path) {
    let ks = KillSwitch::new(state_dir);
    if ks.saved() && ks.resume().is_err() {
        eprintln!("marzvpn-helper: could not restore the kill switch");
    }
}
