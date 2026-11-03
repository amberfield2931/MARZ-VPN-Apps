//! HTTPS client for the MARZ App API, behaviour of Android RC6 HttpAppApi. Every session is bound to a
//! P-256 device key; refresh, logout and login completion are signed; an access token refreshes once on
//! 401. No log line anywhere in this module: tokens, ids and bodies have no business in one.
use crate::checkout;
use crate::http::Endpoint;
use crate::model::{Account, Server, Subscription};
use crate::refresh::{self, AuthClock, AuthStore, Cancel, Outcome, RefreshClient, RefreshTransport, Session, State, Tokens};
use crate::vault::{self, Vault};
use base64::Engine;
use p256::ecdsa::signature::Signer;
use p256::ecdsa::{Signature, SigningKey};
use p256::pkcs8::{DecodePrivateKey, EncodePrivateKey, EncodePublicKey};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

pub const PRODUCTION_URL: &str = "https://app-api.marzvpn.com";
pub const PLATFORM: &str = "desktop";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    /// The API's stable error string, or "network" / "signed_out" / "storage_unavailable" from the client.
    pub code: String,
    pub status: u16,
}

impl ApiError {
    fn local(code: &str) -> Self {
        ApiError { code: code.into(), status: 0 }
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

struct Response {
    code: u16,
    body: Vec<u8>,
}

impl Response {
    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or(Value::Null)
    }
    fn error(&self) -> String {
        self.json().get("error").and_then(Value::as_str).map_or_else(|| format!("http_{}", self.code), str::to_string)
    }
    fn fail<T>(&self) -> ApiResult<T> {
        Err(ApiError { code: self.error(), status: self.code })
    }
    fn local(code: u16, error: &str) -> Self {
        Response { code, body: json!({"error": error}).to_string().into_bytes() }
    }
}

// ---- device identity ------------------------------------------------------------------------------

pub struct DeviceKey {
    key: SigningKey,
    pub id: String,
}

impl DeviceKey {
    /// The install's identity, created once. None when the store cannot keep it.
    fn load(vault: &Vault) -> Option<Self> {
        if let Some(d) = vault.get(vault::DEVICE) {
            let der = base64::engine::general_purpose::STANDARD.decode(d.get("key")?.as_str()?).ok()?;
            let key = SigningKey::from_pkcs8_der(&der).ok()?;
            return Some(DeviceKey { key, id: d.get("id")?.as_str()?.to_string() });
        }
        let mut seed = [0u8; 32];
        getrandom::getrandom(&mut seed).ok()?;
        let key = SigningKey::from_slice(&seed).ok()?;
        let der = key.to_pkcs8_der().ok()?;
        let id = random_uuid()?;
        let record = json!({"key": base64::engine::general_purpose::STANDARD.encode(der.as_bytes()), "id": id});
        vault.put(vault::DEVICE, Some(record)).then_some(DeviceKey { key, id })
    }

    /// X.509 SubjectPublicKeyInfo (DER), base64: the format the App API expects.
    pub fn public_key(&self) -> Option<String> {
        let der = self.key.verifying_key().to_public_key_der().ok()?;
        Some(base64::engine::general_purpose::STANDARD.encode(der.as_bytes()))
    }

    pub fn sign(&self, message: &[u8]) -> String {
        let sig: Signature = self.key.sign(message);
        base64::engine::general_purpose::STANDARD.encode(sig.to_der().as_bytes())
    }
}

fn random_uuid() -> Option<String> {
    let mut b = [0u8; 16];
    getrandom::getrandom(&mut b).ok()?;
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    Some(format!("{}-{}-{}-{}-{}", &h[..8], &h[8..12], &h[12..16], &h[16..20], &h[20..]))
}

pub fn random_url_safe(bytes: usize) -> String {
    let mut b = vec![0u8; bytes];
    getrandom::getrandom(&mut b).expect("system randomness");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
}

pub fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data).iter().map(|b| format!("{b:02x}")).collect()
}

fn unix_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// The maker and model, as RC6 names a phone; never the hostname, which often holds a person's name.
fn device_name() -> String {
    #[cfg(target_os = "linux")]
    {
        let read = |f: &str| std::fs::read_to_string(format!("/sys/class/dmi/id/{f}")).map(|s| s.trim().to_string()).unwrap_or_default();
        let name = format!("{} {}", read("sys_vendor"), read("product_name"));
        let name = name.trim();
        if !name.is_empty() && !name.to_ascii_lowercase().contains("to be filled") {
            return name.chars().take(60).collect();
        }
        "Linux PC".into()
    }
    #[cfg(not(target_os = "linux"))]
    {
        "Windows PC".into()
    }
}

// ---- durable session ------------------------------------------------------------------------------

struct VaultAuthStore(Arc<Vault>);

impl AuthStore for VaultAuthStore {
    fn read(&self) -> State {
        let session = self.0.get(vault::TOKENS).and_then(|v| serde_json::from_value::<Session>(v).ok());
        let cancels = self.0.get(vault::REFRESH_CANCELS).and_then(|v| serde_json::from_value::<Vec<Cancel>>(v).ok()).unwrap_or_default();
        State { session, cancels }
    }

    fn write(&self, state: &State) -> bool {
        let session = state.session.as_ref().and_then(|s| serde_json::to_value(s).ok());
        let cancels = (!state.cancels.is_empty()).then(|| serde_json::to_value(&state.cancels).ok()).flatten();
        self.0.commit(&[(vault::TOKENS, session), (vault::REFRESH_CANCELS, cancels)])
    }
}

struct Signed {
    endpoint: Endpoint,
    device: Option<Arc<DeviceKey>>,
    last_ts: AtomicI64,
}

impl RefreshTransport for Arc<Signed> {
    fn post(&self, path: &str, parent: &str, id: &str, before_body: &mut dyn FnMut() -> bool) -> Result<refresh::Reply, ()> {
        let device = self.device.as_ref().ok_or(())?;
        let bytes = json!({"refresh_token": parent, "request_id": id}).to_string().into_bytes();
        // Every attempt gets its own timestamp, so its own signature (a processed proof is never reused).
        let mut ts = unix_now();
        while ts <= self.last_ts.load(Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_millis(250));
            ts = unix_now();
        }
        self.last_ts.store(ts, Ordering::SeqCst);
        let message = format!("POST\n{path}\n{ts}\n{}", sha256_hex(&bytes));
        let headers = [
            ("Accept", "application/json".to_string()),
            ("Content-Type", "application/json".to_string()),
            ("X-App-Platform", PLATFORM.to_string()),
            ("X-App-Version", env!("CARGO_PKG_VERSION").to_string()),
            ("X-Device-Timestamp", ts.to_string()),
            ("X-Device-Signature", device.sign(message.as_bytes())),
        ];
        let r = self.endpoint.request("POST", path, &headers, &bytes, Some(before_body)).map_err(|_| ())?;
        let j: Value = serde_json::from_slice(&r.body).unwrap_or(Value::Null);
        let str_of = |k: &str| j.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let tokens = (r.code == 200)
            .then(|| Tokens { access: str_of("access_token"), refresh: str_of("refresh_token"), session: str_of("session_id") })
            .filter(|t| !t.access.is_empty() && !t.refresh.is_empty());
        Ok(refresh::Reply {
            code: r.code,
            error: j.get("error").and_then(Value::as_str).map_or_else(|| format!("http_{}", r.code), str::to_string),
            tokens,
            retry_after: j.get("retry_after").and_then(Value::as_u64).filter(|v| *v > 0).map(|v| v as u32),
        })
    }
}

pub struct SystemClock;

impl AuthClock for SystemClock {
    fn wall(&self) -> i64 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
    }

    fn elapsed(&self) -> i64 {
        #[cfg(unix)]
        {
            let mut t = libc::timespec { tv_sec: 0, tv_nsec: 0 };
            #[cfg(target_os = "linux")]
            let clock = libc::CLOCK_BOOTTIME;
            #[cfg(not(target_os = "linux"))]
            let clock = libc::CLOCK_MONOTONIC;
            unsafe { libc::clock_gettime(clock, &mut t) };
            t.tv_sec * 1000 + t.tv_nsec / 1_000_000
        }
        #[cfg(windows)]
        {
            (unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount64() }) as i64
        }
    }

    fn boot(&self) -> i64 {
        #[cfg(target_os = "linux")]
        {
            let Ok(id) = std::fs::read_to_string("/proc/sys/kernel/random/boot_id") else { return -1 };
            let digest = Sha256::digest(id.trim().as_bytes());
            let mut b = [0u8; 8];
            b.copy_from_slice(&digest[..8]);
            (u64::from_be_bytes(b) >> 1) as i64
        }
        #[cfg(not(target_os = "linux"))]
        {
            -1
        }
    }

    fn sleep(&self, ms: u64) {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
}

// ---- API -------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelegramLogin {
    pub telegram_url: String,
    pub match_code: String,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginProgress {
    Pending,
    Done,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckoutIntent {
    pub channel: String,
    pub url: String,
    pub expires_in: u64,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckoutReceipt {
    pub id: String,
    pub profile_id: String,
    pub channel: String,
    pub state: String,
}

pub struct AppApi {
    endpoint: Endpoint,
    vault: Arc<Vault>,
    device: Option<Arc<DeviceKey>>,
    signed: Arc<Signed>,
    auth: RefreshClient<VaultAuthStore, Arc<Signed>, SystemClock>,
}

impl AppApi {
    pub fn new(endpoint: Endpoint, vault: Arc<Vault>) -> Self {
        let device = DeviceKey::load(&vault).map(Arc::new);
        let signed = Arc::new(Signed { endpoint: endpoint.clone(), device: device.clone(), last_ts: AtomicI64::new(0) });
        let auth = RefreshClient::new(VaultAuthStore(vault.clone()), signed.clone(), SystemClock, Box::new(|| random_url_safe(32)));
        AppApi { endpoint, vault, device, signed, auth }
    }

    pub fn vault(&self) -> &Arc<Vault> {
        &self.vault
    }

    // ---- transport ----------------------------------------------------------------------------------

    fn send(&self, method: &str, path: &str, body: Option<&Value>, bearer: Option<&str>, signed: bool) -> std::io::Result<Response> {
        let bytes = body.map_or_else(Vec::new, |b| b.to_string().into_bytes());
        let mut headers = vec![
            ("Accept", "application/json".to_string()),
            ("X-App-Platform", PLATFORM.to_string()),
            ("X-App-Version", env!("CARGO_PKG_VERSION").to_string()),
        ];
        if let Some(b) = bearer {
            headers.push(("Authorization", format!("Bearer {b}")));
        }
        if signed {
            let device = self.device.as_ref().ok_or_else(|| std::io::Error::other("no device key"))?;
            let ts = unix_now().to_string();
            let path_only = path.split('?').next().unwrap_or(path);
            let message = format!("{method}\n{path_only}\n{ts}\n{}", sha256_hex(&bytes));
            headers.push(("X-Device-Timestamp", ts));
            headers.push(("X-Device-Signature", device.sign(message.as_bytes())));
        }
        if method != "GET" {
            headers.push(("Content-Type", "application/json".to_string()));
        }
        let r = self.endpoint.request(method, path, &headers, &bytes, None)?;
        Ok(Response { code: r.code, body: r.body })
    }

    fn call(&self, method: &str, path: &str, body: Option<&Value>, auth: bool, signed: bool) -> Response {
        let run = || -> std::io::Result<Response> {
            if !auth {
                return self.send(method, path, body, None, signed);
            }
            let Some(access) = self.auth.access() else { return Ok(Response::local(401, "signed_out")) };
            let first = self.send(method, path, body, Some(&access), signed)?;
            if first.code != 401 {
                return Ok(first);
            }
            Ok(match self.auth.refresh(&access) {
                Outcome::Done => match self.auth.access() {
                    Some(again) => self.send(method, path, body, Some(&again), signed)?,
                    None => Response::local(401, "signed_out"),
                },
                Outcome::Rejected => {
                    self.vault.clear_session(); // the server ended the session: account data goes too
                    Response::local(401, "signed_out")
                }
                // The rotation may have happened and cannot be recovered: sign in again (data and VPN stay).
                Outcome::Reauth => Response::local(401, "reauth_required"),
                // Not a sign-out: the session, snapshot and VPN config stay; a later sync retries.
                Outcome::Transient(r) => Response::local(r.code, &r.error),
            })
        };
        run().unwrap_or_else(|_| Response::local(0, "network"))
    }

    fn device_json(&self) -> Result<Value, ApiError> {
        let d = self.device.as_ref().ok_or_else(|| ApiError::local("storage_unavailable"))?;
        let public_key = d.public_key().ok_or_else(|| ApiError::local("storage_unavailable"))?;
        Ok(json!({"id": d.id, "name": device_name(), "public_key": public_key, "platform": PLATFORM}))
    }

    fn save_tokens(&self, j: &Value) -> ApiResult<()> {
        let field = |k: &str| j.get(k).and_then(Value::as_str).filter(|s| !s.is_empty());
        let (Some(access), Some(refresh)) = (field("access_token"), field("refresh_token")) else {
            return Err(ApiError::local("invalid_response"));
        };
        let t = Tokens { access: access.into(), refresh: refresh.into(), session: field("session_id").unwrap_or("").into() };
        if self.auth.save_sign_in(&t) {
            Ok(())
        } else {
            Err(ApiError::local("storage_unavailable"))
        }
    }

    // ---- auth ---------------------------------------------------------------------------------------

    pub fn is_signed_in(&self) -> bool {
        self.auth.is_signed_in()
    }

    pub fn start_telegram_login(&self) -> ApiResult<TelegramLogin> {
        let verifier = random_url_safe(48);
        let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        let body = json!({"device": self.device_json()?, "code_challenge": challenge});
        let r = self.call("POST", "/v1/auth/telegram/start", Some(&body), false, false);
        if r.code != 200 {
            return r.fail();
        }
        let j = r.json();
        let bad = || ApiError { code: "invalid_response".into(), status: r.code };
        let login_id = j.get("login_id").and_then(Value::as_str).ok_or_else(bad)?;
        let telegram_url = j.get("telegram_url").and_then(Value::as_str).ok_or_else(bad)?;
        let match_code = j.get("match_code").and_then(Value::as_str).unwrap_or("");
        let expires_at = unix_now() + j.get("expires_in").and_then(Value::as_i64).unwrap_or(300);
        // Kept with the request: returning from Telegram or an app restart shows the same number and
        // keeps waiting for the same request.
        let saved = self.vault.put(
            vault::PENDING_TELEGRAM,
            Some(json!({"login_id": login_id, "verifier": verifier, "expires_at": expires_at, "telegram_url": telegram_url, "match_code": match_code})),
        );
        if !saved {
            return Err(ApiError::local("storage_unavailable"));
        }
        Ok(TelegramLogin { telegram_url: telegram_url.into(), match_code: match_code.into(), expires_at })
    }

    pub fn pending_telegram_login(&self) -> Option<TelegramLogin> {
        let p = self.vault.get(vault::PENDING_TELEGRAM)?;
        Some(TelegramLogin {
            telegram_url: p.get("telegram_url")?.as_str().filter(|s| !s.is_empty())?.into(),
            match_code: p.get("match_code")?.as_str().filter(|s| !s.is_empty())?.into(),
            expires_at: p.get("expires_at")?.as_i64()?,
        })
    }

    pub fn poll_telegram_login(&self) -> ApiResult<LoginProgress> {
        let p = self.vault.get(vault::PENDING_TELEGRAM).ok_or_else(|| ApiError::local("login_not_found"))?;
        let body = json!({"login_id": p.get("login_id"), "code_verifier": p.get("verifier")});
        let r = self.call("POST", "/v1/auth/telegram/confirm", Some(&body), false, true);
        match r.code {
            202 => Ok(LoginProgress::Pending),
            200 => {
                self.save_tokens(&r.json())?;
                self.vault.put(vault::PENDING_TELEGRAM, None);
                Ok(LoginProgress::Done)
            }
            0 => r.fail(),
            _ => {
                self.vault.put(vault::PENDING_TELEGRAM, None);
                r.fail()
            }
        }
    }

    pub fn cancel_telegram_login(&self) {
        self.vault.put(vault::PENDING_TELEGRAM, None);
    }

    pub fn start_email_login(&self, email: &str, locale: &str) -> ApiResult<()> {
        let r = self.call("POST", "/v1/auth/email/start", Some(&json!({"email": email, "locale": locale})), false, false);
        if r.code == 202 {
            Ok(())
        } else {
            r.fail()
        }
    }

    pub fn verify_email_code(&self, email: &str, code: &str) -> ApiResult<()> {
        let body = json!({"email": email, "code": code, "device": self.device_json()?});
        let r = self.call("POST", "/v1/auth/email/verify", Some(&body), false, true);
        if r.code != 200 {
            return r.fail();
        }
        self.save_tokens(&r.json())
    }

    pub fn start_email_link(&self, email: &str, locale: &str) -> ApiResult<()> {
        let r = self.call("POST", "/v1/account/identities/email/start", Some(&json!({"email": email, "locale": locale})), true, true);
        if r.code == 202 {
            Ok(())
        } else {
            r.fail()
        }
    }

    pub fn confirm_email_link(&self, email: &str, code: &str) -> ApiResult<Account> {
        let r = self.call("POST", "/v1/account/identities/email/confirm", Some(&json!({"email": email, "code": code})), true, true);
        if r.code != 200 {
            return r.fail();
        }
        // A link returns an account view, never a replacement access/refresh pair.
        Account::parse(&r.json()).ok_or(ApiError { code: "invalid_response".into(), status: 200 })
    }

    pub fn unlink_email(&self, identity_id: &str) -> ApiResult<()> {
        if identity_id.is_empty() || identity_id.len() > 100 || !identity_id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
            return Err(ApiError::local("invalid_identity"));
        }
        let r = self.call("DELETE", &format!("/v1/account/identities/email/{identity_id}"), None, true, true);
        if r.code == 200 {
            Ok(())
        } else {
            r.fail()
        }
    }

    /// Signed out here at once (also offline); then the server revokes the session.
    pub fn logout(&self) {
        self.auth.logout(&|access| {
            self.send("POST", "/v1/auth/logout", Some(&json!({})), Some(access), true).map_or(0, |r| r.code)
        });
        self.vault.clear_session();
    }

    pub fn deliver_pending_cancellations(&self) {
        self.auth.deliver_cancels();
    }

    // ---- data ---------------------------------------------------------------------------------------

    pub fn account(&self) -> ApiResult<Account> {
        let r = self.call("GET", "/v1/account", None, true, false);
        if r.code != 200 {
            return r.fail();
        }
        let j = r.json();
        let parsed = Account::parse(&j).ok_or(ApiError { code: "invalid_response".into(), status: 200 })?;
        self.vault.put(vault::CACHE_ACCOUNT, Some(j));
        Ok(parsed)
    }

    pub fn subscription(&self) -> ApiResult<Subscription> {
        let r = self.call("GET", "/v1/subscription", None, true, false);
        if r.code != 200 {
            return r.fail();
        }
        let j = r.json();
        self.vault.put(vault::CACHE_SUBSCRIPTION, Some(j.clone()));
        Ok(Subscription::parse(&j))
    }

    /// Last account + subscription seen online, for showing state offline.
    pub fn offline_snapshot(&self) -> Option<(Account, Subscription)> {
        if !self.is_signed_in() {
            return None;
        }
        let a = self.vault.get(vault::CACHE_ACCOUNT)?;
        let s = self.vault.get(vault::CACHE_SUBSCRIPTION)?;
        Some((Account::parse(&a)?, Subscription::parse(&s)))
    }

    pub fn servers(&self) -> ApiResult<Vec<Server>> {
        let r = self.call("GET", "/v1/servers", None, true, false);
        if r.code == 403 {
            return Ok(Vec::new()); // no active plan: nothing to choose
        }
        if r.code != 200 {
            return r.fail();
        }
        let j = r.json();
        Ok(j.get("servers").and_then(Value::as_array).map_or(Vec::new(), |a| a.iter().filter_map(Server::parse).collect()))
    }

    pub fn vpn_config(&self, server_id: Option<&str>) -> ApiResult<String> {
        let query = server_id.map_or_else(String::new, |s| {
            let clean: String = s.chars().filter(char::is_ascii_alphanumeric).collect();
            format!("?server={clean}")
        });
        let r = self.call("GET", &format!("/v1/vpn/config{query}"), None, true, false);
        if r.code != 200 {
            return r.fail();
        }
        String::from_utf8(r.body).map_err(|_| ApiError { code: "invalid_response".into(), status: 200 })
    }

    pub fn create_checkout_intent(&self, plan: &str, action: &str, vpn_identity: Option<&str>, locale: &str, channel: Option<&str>) -> ApiResult<CheckoutIntent> {
        let mut body = json!({"action": action, "plan": plan, "locale": locale});
        if let Some(v) = vpn_identity {
            body["vpn_identity"] = json!(v);
        }
        if let Some(c) = channel {
            body["channel"] = json!(c);
        }
        let r = self.call("POST", "/v1/billing/intent", Some(&body), true, true);
        if r.code != 200 {
            return r.fail();
        }
        let j = r.json();
        let text = |k: &str| j.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let (got_channel, url, id) = (text("channel"), text("url"), text("id"));
        // The answer must be for exactly this request, and the link one of the two exact shapes.
        let valid = checkout::validate_link(&got_channel, &url, plan, locale)
            && channel.is_none_or(|c| c == got_channel)
            && checkout::is_operation(&id)
            && text("plan") == plan
            && text("action") == action;
        if !valid {
            return Err(ApiError { code: "invalid_handoff".into(), status: r.code });
        }
        Ok(CheckoutIntent { channel: got_channel, url, expires_in: j.get("expires_in").and_then(Value::as_u64).unwrap_or(120).clamp(1, 120), id })
    }

    pub fn checkout_status(&self, id: Option<&str>) -> ApiResult<Option<CheckoutReceipt>> {
        let body = id.map_or_else(|| json!({}), |i| json!({"id": i}));
        let r = self.call("POST", if id.is_none() { "/v1/billing/current" } else { "/v1/billing/status" }, Some(&body), true, true);
        if r.code != 200 {
            return r.fail();
        }
        let j = r.json();
        let text = |k: &str| j.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        if id.is_none() && text("state") == "none" {
            return Ok(None);
        }
        let receipt = CheckoutReceipt { id: text("id"), profile_id: text("vpn_identity"), channel: text("channel"), state: text("state") };
        let valid = matches!(receipt.channel.as_str(), "website" | "telegram_bot")
            && checkout::is_operation(&receipt.id)
            && !receipt.profile_id.trim().is_empty()
            && checkout::STATES.contains(&receipt.state.as_str())
            && id.is_none_or(|i| i == receipt.id);
        if !valid {
            return Err(ApiError::local("invalid_payment_status"));
        }
        Ok(Some(receipt))
    }

    /// Public catalog; prices are never hard-coded on top of what the server states.
    pub fn catalog(&self) -> ApiResult<Value> {
        let r = self.call("GET", "/v1/catalog", None, false, false);
        if r.code == 200 {
            Ok(r.json())
        } else {
            r.fail()
        }
    }

    pub fn capabilities(&self) -> ApiResult<Value> {
        let r = self.call("GET", "/v1/capabilities", None, false, false);
        if r.code == 200 {
            Ok(r.json())
        } else {
            r.fail()
        }
    }

    #[doc(hidden)]
    pub fn signed_transport_for_tests(&self) -> bool {
        self.signed.device.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::FixedKey;

    fn vault(name: &str) -> Arc<Vault> {
        let d = std::env::temp_dir().join(format!("marz-api-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        Arc::new(Vault::open(&d, &FixedKey([3; 32])))
    }

    #[test]
    fn device_key_is_stable_and_public_key_is_spki() {
        let v = vault("key");
        let a = DeviceKey::load(&v).unwrap();
        let b = DeviceKey::load(&v).unwrap();
        assert_eq!(a.id, b.id);
        assert_eq!(a.public_key(), b.public_key());
        let der = base64::engine::general_purpose::STANDARD.decode(a.public_key().unwrap()).unwrap();
        assert_eq!(der[0], 0x30); // SEQUENCE
        assert_eq!(der.len(), 91); // P-256 SubjectPublicKeyInfo
    }

    #[test]
    fn signatures_are_der_ecdsa() {
        let v = vault("sig");
        let k = DeviceKey::load(&v).unwrap();
        let sig = base64::engine::general_purpose::STANDARD.decode(k.sign(b"m")).unwrap();
        assert_eq!(sig[0], 0x30);
        assert!(Signature::from_der(&sig).is_ok());
    }

    #[test]
    fn device_id_matches_the_server_rule() {
        let id = random_uuid().unwrap();
        assert!((8..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'));
    }

    #[test]
    fn device_name_never_uses_the_hostname() {
        let host = std::fs::read_to_string("/etc/hostname").unwrap_or_default();
        let host = host.trim();
        if host.len() > 3 {
            assert!(!device_name().to_ascii_lowercase().contains(&host.to_ascii_lowercase()));
        }
    }

    #[test]
    fn no_session_means_signed_out_without_a_request() {
        let v = vault("out");
        let config = crate::tls::client_config_with(rustls::RootCertStore::empty()).unwrap();
        let api = AppApi::new(Endpoint::parse_with("https://127.0.0.1:1", config).unwrap(), v);
        assert_eq!(api.account().unwrap_err().code, "signed_out");
        assert!(!api.is_signed_in());
    }

    #[test]
    fn unusable_store_blocks_sign_in() {
        struct No;
        impl crate::vault::KeySource for No {
            fn load(&self) -> Result<[u8; 32], crate::vault::KeyError> {
                Err(crate::vault::KeyError::Unavailable)
            }
            fn save(&self, _: &[u8; 32]) -> Result<(), crate::vault::KeyError> {
                Err(crate::vault::KeyError::Unavailable)
            }
        }
        let d = std::env::temp_dir().join(format!("marz-api-test-{}-no", std::process::id()));
        let config = crate::tls::client_config_with(rustls::RootCertStore::empty()).unwrap();
        let api = AppApi::new(Endpoint::parse_with("https://127.0.0.1:1", config).unwrap(), Arc::new(Vault::open(&d, &No)));
        assert_eq!(api.start_telegram_login().unwrap_err().code, "storage_unavailable");
    }
}
