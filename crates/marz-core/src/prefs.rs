//! Non-secret settings, same set as RC6 MarzPrefs. A damaged file reads as defaults.
use crate::model::Server;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Prefs {
    /// system | light | dark
    pub theme: String,
    /// system | ru | en | fa | zh-CN
    pub language: String,
    pub welcome_seen: bool,
    /// None = Automatic.
    pub server: Option<String>,
    pub routes: Option<Vec<Server>>,
    /// Block direct traffic whenever the VPN switch is ON. A file from before this setting reads as on.
    pub kill_switch: bool,
    /// When the update check last ran, Unix seconds. 0: never.
    pub update_checked_at: i64,
    /// A newer published version found by the last check, shown until a check says otherwise.
    pub update_latest: Option<String>,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs { theme: "system".into(), language: "system".into(), welcome_seen: false, server: None, routes: None, kill_switch: true, update_checked_at: 0, update_latest: None }
    }
}

pub struct PrefsFile {
    path: PathBuf,
}

impl PrefsFile {
    pub fn new(dir: &Path) -> Self {
        PrefsFile { path: dir.join("prefs.json") }
    }

    pub fn load(&self) -> Prefs {
        std::fs::read(&self.path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&self, prefs: &Prefs) {
        let Ok(text) = serde_json::to_vec_pretty(prefs) else { return };
        if let Some(d) = self.path.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let tmp = self.path.with_extension("tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, &self.path);
        }
    }
}
