//! Encrypted store for the session, device key and account cache: one AES-256-GCM file whose key lives
//! in the OS keyring (Secret Service on Linux, Credential Manager on Windows). Without a working keyring
//! nothing is saved: a sign-in then fails with `storage_unavailable`, never falls back to plain text.
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use serde_json::{Map, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const TOKENS: &str = "tokens";
pub const REFRESH_CANCELS: &str = "refresh_cancels";
pub const PENDING_TELEGRAM: &str = "pending_telegram";
pub const CACHE_ACCOUNT: &str = "cache_account";
pub const CACHE_SUBSCRIPTION: &str = "cache_subscription";
pub const ENGINE_CONFIG: &str = "engine_config";
pub const DEVICE: &str = "device";

const HEADER: &[u8] = b"MARZVAULT1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyError {
    /// There is no key yet (first run, or the keyring was reset): a new one may be created.
    Missing,
    /// The keyring cannot be used now (locked, no daemon): existing data must not be overwritten.
    Unavailable,
}

pub trait KeySource: Send + Sync {
    fn load(&self) -> Result<[u8; 32], KeyError>;
    fn save(&self, key: &[u8; 32]) -> Result<(), KeyError>;
}

#[cfg(feature = "os-keyring")]
pub struct OsKeyring;

#[cfg(feature = "os-keyring")]
impl OsKeyring {
    fn entry() -> Result<keyring::Entry, KeyError> {
        keyring::Entry::new("MARZ VPN", "vault-key-v1").map_err(|_| KeyError::Unavailable)
    }
}

#[cfg(feature = "os-keyring")]
impl KeySource for OsKeyring {
    fn load(&self) -> Result<[u8; 32], KeyError> {
        match Self::entry()?.get_secret() {
            Ok(bytes) => bytes.try_into().map_err(|_| KeyError::Missing),
            Err(keyring::Error::NoEntry) => Err(KeyError::Missing),
            Err(_) => Err(KeyError::Unavailable),
        }
    }

    fn save(&self, key: &[u8; 32]) -> Result<(), KeyError> {
        Self::entry()?.set_secret(key).map_err(|_| KeyError::Unavailable)
    }
}

pub struct FixedKey(pub [u8; 32]);

impl KeySource for FixedKey {
    fn load(&self) -> Result<[u8; 32], KeyError> {
        Ok(self.0)
    }
    fn save(&self, _: &[u8; 32]) -> Result<(), KeyError> {
        Ok(())
    }
}

/// `key` and `records` are absent while the store is unusable; every write then reports failure.
pub struct Vault {
    path: PathBuf,
    state: Mutex<State>,
}

struct State {
    key: Option<[u8; 32]>,
    records: Map<String, Value>,
}

impl Vault {
    pub fn open(dir: &Path, source: &dyn KeySource) -> Self {
        let path = dir.join("vault.bin");
        let key = match source.load() {
            Ok(k) => Some(k),
            Err(KeyError::Unavailable) => None,
            Err(KeyError::Missing) => new_key().filter(|k| source.save(k).is_ok()),
        };
        let records = key.as_ref().and_then(|k| read_file(&path, k)).unwrap_or_default();
        Vault { path, state: Mutex::new(State { key, records }) }
    }

    pub fn usable(&self) -> bool {
        self.state.lock().unwrap().key.is_some()
    }

    pub fn get(&self, name: &str) -> Option<Value> {
        self.state.lock().unwrap().records.get(name).cloned()
    }

    /// Several records changed in one durable step; true only once they are on disk. Null removes one.
    pub fn commit(&self, changes: &[(&str, Option<Value>)]) -> bool {
        let mut g = self.state.lock().unwrap();
        let Some(key) = g.key else { return false };
        let mut next = g.records.clone();
        for (name, value) in changes {
            match value {
                Some(v) => next.insert((*name).to_string(), v.clone()),
                None => next.remove(*name),
            };
        }
        if write_file(&self.path, &key, &next).is_err() {
            return false;
        }
        g.records = next;
        true
    }

    pub fn put(&self, name: &str, value: Option<Value>) -> bool {
        self.commit(&[(name, value)])
    }

    /// Sign-out: account data goes; the device identity stays, as on Android.
    pub fn clear_session(&self) -> bool {
        self.commit(&[
            (TOKENS, None),
            (PENDING_TELEGRAM, None),
            (CACHE_ACCOUNT, None),
            (CACHE_SUBSCRIPTION, None),
            (ENGINE_CONFIG, None),
        ])
    }
}

fn new_key() -> Option<[u8; 32]> {
    let mut k = [0u8; 32];
    getrandom::getrandom(&mut k).ok()?;
    Some(k)
}

fn read_file(path: &Path, key: &[u8; 32]) -> Option<Map<String, Value>> {
    let data = std::fs::read(path).ok()?;
    let rest = data.strip_prefix(HEADER)?;
    if rest.len() < 12 {
        return None;
    }
    let (nonce, sealed) = rest.split_at(12);
    let plain = Aes256Gcm::new(key.into()).decrypt(Nonce::from_slice(nonce), sealed).ok()?;
    serde_json::from_slice(&plain).ok()
}

/// Temp file, fsync, rename: a crash leaves the old or the new vault, never a partial one.
fn write_file(path: &Path, key: &[u8; 32], records: &Map<String, Value>) -> std::io::Result<()> {
    let plain = serde_json::to_vec(records)?;
    let mut nonce = [0u8; 12];
    getrandom::getrandom(&mut nonce).map_err(std::io::Error::other)?;
    let sealed = Aes256Gcm::new(key.into()).encrypt(Nonce::from_slice(&nonce), plain.as_slice()).map_err(|_| std::io::ErrorKind::Other)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
        restrict_dir(dir);
    }
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    let mut file = open_private(&tmp)?;
    file.write_all(HEADER)?;
    file.write_all(&nonce)?;
    file.write_all(&sealed)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&tmp, path)
}

#[cfg(unix)]
fn open_private(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)
}

#[cfg(unix)]
fn restrict_dir(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
}

#[cfg(not(unix))]
fn open_private(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new().write(true).create(true).truncate(true).open(path)
}

#[cfg(not(unix))]
fn restrict_dir(_: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("marz-vault-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    struct Failing(KeyError);
    impl KeySource for Failing {
        fn load(&self) -> Result<[u8; 32], KeyError> {
            Err(self.0)
        }
        fn save(&self, _: &[u8; 32]) -> Result<(), KeyError> {
            Err(KeyError::Unavailable)
        }
    }

    #[test]
    fn round_trip_is_encrypted_on_disk() {
        let d = dir("a");
        let k = FixedKey([7; 32]);
        let v = Vault::open(&d, &k);
        assert!(v.put(TOKENS, Some(json!({"refresh": "SECRET-TOKEN"}))));
        let raw = std::fs::read(d.join("vault.bin")).unwrap();
        assert!(!raw.windows(12).any(|w| w == b"SECRET-TOKEN"));
        let again = Vault::open(&d, &k);
        assert_eq!(again.get(TOKENS).unwrap()["refresh"], "SECRET-TOKEN");
    }

    #[cfg(unix)]
    #[test]
    fn file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let d = dir("p");
        let v = Vault::open(&d, &FixedKey([7; 32]));
        assert!(v.put(TOKENS, Some(json!(1))));
        assert_eq!(std::fs::metadata(d.join("vault.bin")).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::metadata(&d).unwrap().permissions().mode() & 0o777, 0o700);
    }

    #[test]
    fn wrong_key_or_tampered_file_reads_as_empty() {
        let d = dir("b");
        let v = Vault::open(&d, &FixedKey([7; 32]));
        assert!(v.put(TOKENS, Some(json!(1))));
        assert!(Vault::open(&d, &FixedKey([8; 32])).get(TOKENS).is_none());
        let mut raw = std::fs::read(d.join("vault.bin")).unwrap();
        let last = raw.len() - 1;
        raw[last] ^= 1;
        std::fs::write(d.join("vault.bin"), raw).unwrap();
        assert!(Vault::open(&d, &FixedKey([7; 32])).get(TOKENS).is_none());
    }

    #[test]
    fn no_keyring_means_nothing_is_written() {
        let d = dir("c");
        let v = Vault::open(&d, &Failing(KeyError::Unavailable));
        assert!(!v.usable());
        assert!(!v.put(TOKENS, Some(json!("x"))));
        assert!(!d.join("vault.bin").exists());
    }

    #[test]
    fn keyring_that_refuses_a_new_key_means_unusable() {
        let d = dir("d");
        assert!(!Vault::open(&d, &Failing(KeyError::Missing)).usable());
    }

    #[test]
    fn clear_session_keeps_the_device_identity() {
        let d = dir("e");
        let v = Vault::open(&d, &FixedKey([1; 32]));
        assert!(v.commit(&[(TOKENS, Some(json!(1))), (DEVICE, Some(json!(2))), (ENGINE_CONFIG, Some(json!(3)))]));
        assert!(v.clear_session());
        assert!(v.get(TOKENS).is_none() && v.get(ENGINE_CONFIG).is_none());
        assert_eq!(v.get(DEVICE).unwrap(), 2);
    }
}
