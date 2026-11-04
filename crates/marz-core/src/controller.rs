//! The app's behaviour, independent of any window: RC6 MarzViewModel for the desktop. The UI only reads
//! `view()` and calls actions, so all of this runs under test without a display.
//!
//! The rule the owner cares about most: the VPN switch is the user's. Lost networks and engine exits never
//! turn it off (the helper keeps retrying); only the user's button, or the end of the session, does. What the
//! connection really does is shown separately: `traffic` comes from real checks through the tunnel.
use crate::api::{ApiError, AppApi, LoginProgress};
use crate::checkout;
use crate::helper_client::{HelperClient, HelperError};
use crate::model::{migrate_choice, Account, Server, Subscription};
use crate::prefs::{Prefs, PrefsFile};
use crate::tunnel_check::{self, CheckResult};
use crate::vault;
use marz_engine::ipc::{EngineState, KillSwitch, Session, Status, PROTOCOL};
use marz_engine::{EngineRequest, AUTO_TAG};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

pub const EMAIL_RESEND_SECS: i64 = 60;
const STATUS_POLL: Duration = Duration::from_millis(1000);
const SYNC_EVERY: Duration = Duration::from_secs(15 * 60);
const SYNC_MIN_GAP: Duration = Duration::from_secs(60);
const PAYMENT_WATCH: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum TelegramUi {
    Idle,
    Starting,
    Waiting { telegram_url: String, match_code: String, expires_at: i64 },
    Failed { code: String },
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum EmailUi {
    EnterEmail,
    Sending,
    EnterCode { email: String, sent_at: i64, busy: bool, error: Option<String> },
    Failed { code: String },
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum CheckoutUi {
    Idle,
    Requesting,
    Opened { channel: String, profile_id: String, operation_id: String, order: String, checking: bool },
    Confirmed,
    Failed { problem: String, action: String, source: Option<String>, has_profiles: bool },
}

#[derive(Debug, Clone, Serialize)]
pub struct View {
    pub signed_in: bool,
    pub account: Option<Account>,
    pub subscription: Option<Subscription>,
    pub engine: String,
    /// The user's switch.
    pub wanted: bool,
    pub engine_error: Option<String>,
    pub restarts: u32,
    /// The firewall block is up: off | armed. While armed and not connected, the internet is blocked.
    pub kill_switch: String,
    /// none | mine | other: "other" means another user of this computer turned the VPN on.
    pub session: String,
    pub helper_ok: bool,
    /// The service is not the version this app shipped with: a damaged install that must not start a tunnel.
    pub helper_mismatch: bool,
    pub storage_ok: bool,
    /// A config is installed: the button can connect.
    pub config_ready: bool,
    pub active_route: Option<String>,
    /// unknown | ok | lost | no_network
    pub traffic: String,
    pub live_ms: Option<u32>,
    pub servers: Vec<Server>,
    pub from_account: bool,
    pub choice: Option<String>,
    pub route_missing: bool,
    pub availability: BTreeMap<String, String>,
    pub sync_error: Option<String>,
    pub syncing: bool,
    pub telegram: TelegramUi,
    pub email: EmailUi,
    pub checkout: CheckoutUi,
    pub prefs: Prefs,
    pub version: String,
    pub is_test_build: bool,
    /// windows | linux: only for text that must name a system command.
    pub platform: String,
}

struct Inner {
    view: View,
    last_request: Option<(String, String, Option<String>, String)>,
}

pub struct Controller {
    api: AppApi,
    helper: HelperClient,
    prefs_file: PrefsFile,
    inner: Mutex<Inner>,
    on_change: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
    visible: AtomicBool,
    telegram_epoch: AtomicU64,
    route_epoch: AtomicU64,
    check_epoch: AtomicU64,
    last_sync: Mutex<Option<Instant>>,
    sync_lock: Mutex<()>,
    me: Mutex<Weak<Controller>>,
}

fn unix_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// A wrong helper is a damaged install. Windows checks the exact version, since its parts always ship together; a Linux
/// package may carry a helper unchanged from an earlier build, so there only the protocol has to match.
fn helper_mismatch(s: &Status, app_version: &str) -> bool {
    versions_differ(s.protocol, &s.version, app_version, cfg!(windows))
}

fn versions_differ(protocol: u32, helper: &str, app: &str, exact: bool) -> bool {
    protocol != PROTOCOL || (exact && helper != app)
}

fn engine_name(s: EngineState) -> &'static str {
    match s {
        EngineState::Stopped => "disconnected",
        EngineState::Starting => "connecting",
        EngineState::Running => "connected",
        EngineState::Stopping => "disconnecting",
    }
}

fn tag_of(choice: Option<&str>) -> String {
    choice.map_or_else(|| AUTO_TAG.to_string(), |id| format!("srv-{id}"))
}

impl Controller {
    pub fn new(api: AppApi, helper: HelperClient, data_dir: &std::path::Path, version: &str, is_test_build: bool) -> Arc<Self> {
        let prefs_file = PrefsFile::new(data_dir);
        let prefs = prefs_file.load();
        let signed_in = api.is_signed_in();
        let storage_ok = api.vault().usable();
        let (account, subscription) = api.offline_snapshot().map_or((None, None), |(a, s)| (Some(a), Some(s)));
        let cached_routes = if signed_in { prefs.routes.clone() } else { None };
        let view = View {
            signed_in,
            account,
            subscription,
            engine: "unknown".into(),
            wanted: false,
            engine_error: None,
            restarts: 0,
            kill_switch: "off".into(),
            session: "none".into(),
            helper_ok: true,
            helper_mismatch: false,
            storage_ok,
            config_ready: api.vault().get(vault::ENGINE_CONFIG).is_some(),
            active_route: None,
            traffic: "unknown".into(),
            live_ms: None,
            from_account: cached_routes.is_some(),
            servers: cached_routes.unwrap_or_default(),
            choice: prefs.server.clone(),
            route_missing: false,
            availability: BTreeMap::new(),
            sync_error: None,
            syncing: false,
            telegram: TelegramUi::Idle,
            email: EmailUi::EnterEmail,
            checkout: CheckoutUi::Idle,
            prefs,
            version: version.to_string(),
            is_test_build,
            platform: if cfg!(windows) { "windows" } else { "linux" }.to_string(),
        };
        let c = Arc::new(Controller {
            api,
            helper,
            prefs_file,
            inner: Mutex::new(Inner { view, last_request: None }),
            on_change: Mutex::new(None),
            visible: AtomicBool::new(true),
            telegram_epoch: AtomicU64::new(0),
            route_epoch: AtomicU64::new(0),
            check_epoch: AtomicU64::new(0),
            last_sync: Mutex::new(None),
            sync_lock: Mutex::new(()),
            me: Mutex::new(Weak::new()),
        });
        *c.me.lock().unwrap() = Arc::downgrade(&c);
        c
    }

    /// Starts the background loops. Separate from `new` so tests can drive single steps.
    pub fn start(self: &Arc<Self>) {
        self.background(|c| loop {
            c.poll_helper();
            std::thread::sleep(STATUS_POLL);
        });
        self.background(|c| loop {
            let connected = c.read(|v| v.engine == "connected");
            if connected && c.visible.load(Ordering::SeqCst) {
                c.check_tunnel();
            }
            let wait = if c.read(|v| v.traffic == "lost" || v.traffic == "no_network") { 15 } else { 30 };
            let mut slept = 0;
            // Woken early when the engine connects, so the first result comes quickly.
            while slept < wait && (c.read(|v| v.engine == "connected") == connected) {
                std::thread::sleep(Duration::from_secs(1));
                slept += 1;
            }
        });
        self.background(|c| loop {
            if c.api.is_signed_in() {
                c.api.deliver_pending_cancellations();
                c.sync_now();
            }
            let started = Instant::now();
            while started.elapsed() < SYNC_EVERY {
                std::thread::sleep(PAYMENT_WATCH);
                if c.read(|v| matches!(v.checkout, CheckoutUi::Opened { .. })) {
                    c.sync_now();
                }
            }
        });
        if self.api.pending_telegram_login().is_some() {
            self.telegram_resume();
        }
    }

    fn background(self: &Arc<Self>, f: impl FnOnce(Arc<Controller>) + Send + 'static) {
        let c = self.clone();
        std::thread::spawn(move || f(c));
    }

    pub fn set_on_change(&self, f: Box<dyn Fn() + Send + Sync>) {
        *self.on_change.lock().unwrap() = Some(f);
    }

    pub fn view(&self) -> View {
        self.inner.lock().unwrap().view.clone()
    }

    pub fn view_json(&self) -> Value {
        serde_json::to_value(self.view()).unwrap_or(Value::Null)
    }

    fn read<T>(&self, f: impl FnOnce(&View) -> T) -> T {
        f(&self.inner.lock().unwrap().view)
    }

    fn change(&self, f: impl FnOnce(&mut View)) {
        {
            let mut g = self.inner.lock().unwrap();
            f(&mut g.view);
        }
        if let Some(cb) = self.on_change.lock().unwrap().as_ref() {
            cb();
        }
    }

    fn save_prefs(&self) {
        let prefs = self.read(|v| v.prefs.clone());
        self.prefs_file.save(&prefs);
    }

    // ---- helper -------------------------------------------------------------------------------------

    fn apply_status(&self, r: Result<Status, HelperError>) {
        match r {
            Ok(s) => {
                let engine = engine_name(s.state);
                let kill_switch = if s.kill_switch == KillSwitch::Armed { "armed" } else { "off" };
                let session = match s.session {
                    Session::None => "none",
                    Session::Mine => "mine",
                    Session::Other => "other",
                };
                let mismatch = helper_mismatch(&s, self.read(|v| v.version.clone()).as_str());
                if s.needs_request && !mismatch {
                    self.resume_tunnel();
                }
                let same = self.read(|v| {
                    v.helper_ok
                        && v.helper_mismatch == mismatch
                        && v.engine == engine
                        && v.wanted == s.wanted
                        && v.active_route == s.active
                        && v.engine_error == s.error
                        && v.restarts == s.restarts
                        && v.kill_switch == kill_switch
                        && v.session == session
                });
                if same {
                    return;
                }
                let was_connected = self.read(|v| v.engine == "connected");
                self.change(|v| {
                    v.helper_ok = true;
                    v.helper_mismatch = mismatch;
                    v.engine = engine.to_string();
                    v.wanted = s.wanted;
                    v.active_route = s.active.clone();
                    v.engine_error = s.error.clone();
                    v.restarts = s.restarts;
                    v.kill_switch = kill_switch.to_string();
                    v.session = session.to_string();
                    if engine != "connected" {
                        v.traffic = "unknown".into();
                        v.live_ms = None;
                        v.availability.clear();
                    }
                });
                if was_connected != (engine == "connected") {
                    self.check_epoch.fetch_add(1, Ordering::SeqCst);
                }
            }
            Err(HelperError::Unreachable) => {
                // Without the service no state is known: nothing pretends to be connected.
                let same = self.read(|v| !v.helper_ok);
                if !same {
                    self.change(|v| {
                        v.helper_ok = false;
                        v.engine = "unknown".into();
                        v.traffic = "unknown".into();
                        v.live_ms = None;
                        v.active_route = None;
                    });
                }
            }
            Err(HelperError::Refused(code)) => self.change(|v| v.engine_error = Some(code)),
        }
    }

    pub fn poll_helper(&self) {
        self.apply_status(self.helper.status());
    }

    /// The helper restarted (or the computer did) with the switch ON. It keeps blocking but holds no
    /// credentials, so it asks. If there is nothing to send it keeps blocking, and the UI offers OFF.
    fn resume_tunnel(&self) {
        let Some(request) = self.request_for_choice() else { return };
        // Only a block left up asks for this, so the tunnel comes back protected whatever the setting says.
        if let Err(HelperError::Refused(code)) = self.helper.start(request, true) {
            self.change(|v| v.engine_error = Some(code));
        }
    }

    fn request_for_choice(&self) -> Option<EngineRequest> {
        let mut request = self.stored_request()?;
        let choice = self.read(|v| v.choice.clone());
        let tag = tag_of(choice.as_deref());
        if tag == AUTO_TAG || request.routes.iter().any(|r| r.tag == tag) {
            request.selected = tag;
        }
        Some(request)
    }

    /// Asks the service whether this computer is blocked because the switch was left ON. For a launch at
    /// login: not blocked means there is nothing for the app to do.
    pub fn helper_blocking(helper: &HelperClient) -> bool {
        matches!(helper.status(), Ok(s) if s.kill_switch == KillSwitch::Armed)
    }

    fn stored_request(&self) -> Option<EngineRequest> {
        self.api.vault().get(vault::ENGINE_CONFIG).and_then(|v| serde_json::from_value(v).ok())
    }

    fn store_request(&self, r: Option<&EngineRequest>) -> bool {
        let saved = self.api.vault().put(vault::ENGINE_CONFIG, r.and_then(|x| serde_json::to_value(x).ok()));
        let ready = self.api.vault().get(vault::ENGINE_CONFIG).is_some();
        self.change(|v| v.config_ready = ready);
        saved
    }

    /// The big button. Connected or connecting (including waiting to reconnect): off. Off: on.
    pub fn toggle(&self) {
        match self.read(|v| v.engine.clone()).as_str() {
            "connected" | "connecting" | "disconnecting" => {
                let r = self.helper.stop();
                self.apply_status(r);
            }
            "disconnected" => {
                if self.read(|v| v.helper_mismatch) {
                    return;
                }
                let Some(request) = self.request_for_choice() else { return };
                self.change(|v| v.engine_error = None);
                let kill_switch = self.read(|v| v.prefs.kill_switch);
                let r = self.helper.start(request, kill_switch);
                self.apply_status(r);
            }
            _ => {}
        }
    }

    /// Settings. Refused unless the VPN is fully off, so a running tunnel never changes mode.
    pub fn set_kill_switch(&self, on: bool) -> bool {
        let off = self.read(|v| v.helper_ok && v.engine == "disconnected" && !v.wanted);
        if !off {
            return false;
        }
        self.change(|v| v.prefs.kill_switch = on);
        self.save_prefs();
        true
    }

    /// Settings: only after the user confirmed. Ok(false) means the service is not reachable, or this is another
    /// user's session: the caller has to run the recovery with the system's own administrator prompt.
    pub fn emergency_off(&self) -> Result<bool, String> {
        match self.helper.emergency_off() {
            Ok(s) => {
                self.apply_status(Ok(s));
                Ok(true)
            }
            Err(HelperError::Unreachable) => Ok(false),
            Err(HelperError::Refused(code)) if code == marz_engine::auth::NOT_OWNER => Ok(false),
            Err(HelperError::Refused(code)) => Err(code),
        }
    }

    /// After the recovery ran with administrator rights: did it really end the block? The service answers
    /// within a couple of seconds. If it cannot be asked, only the program's own exit code is known.
    pub fn block_is_gone(&self) -> bool {
        for _ in 0..12 {
            match self.helper.status() {
                Ok(s) if s.kill_switch == KillSwitch::Off => {
                    self.apply_status(Ok(s));
                    return true;
                }
                Ok(_) => std::thread::sleep(Duration::from_millis(500)),
                Err(_) => return true,
            }
        }
        false
    }

    // ---- account sync (RC6 syncNow) ------------------------------------------------------------------

    /// Window came to the front (RC6 onForeground): checks run only then, and data refreshes at most once a minute.
    pub fn on_visible(self: &Arc<Self>, visible: bool) {
        self.visible.store(visible, Ordering::SeqCst);
        if !visible {
            return;
        }
        self.background(|c| {
            c.api.deliver_pending_cancellations();
            let due = c.last_sync.lock().unwrap().is_none_or(|t| t.elapsed() >= SYNC_MIN_GAP);
            if due {
                c.sync_now();
            }
        });
    }

    pub fn sync(self: &Arc<Self>) {
        self.background(|c| c.sync_now());
    }

    pub fn sync_now(&self) {
        let Ok(_one_at_a_time) = self.sync_lock.try_lock() else { return };
        if !self.api.is_signed_in() {
            return;
        }
        *self.last_sync.lock().unwrap() = Some(Instant::now());
        self.change(|v| v.syncing = true);
        self.sync_inner();
        self.change(|v| v.syncing = false);
    }

    fn sync_inner(&self) {
        let account = match self.api.account() {
            Ok(a) => a,
            Err(e) => return self.handle_error(e),
        };
        let sub = match self.api.subscription() {
            Ok(s) => s,
            Err(e) => return self.handle_error(e),
        };
        let can_connect = sub.can_connect;
        self.change(|v| {
            v.signed_in = true;
            v.account = Some(account);
            v.subscription = Some(sub);
        });
        self.check_payment();
        let routes = self.api.servers().ok();
        let known = routes.as_ref().is_some_and(|r| !r.is_empty());
        if let Some(list) = routes.filter(|r| !r.is_empty()) {
            let old = self.read(|v| v.servers.clone());
            let migrated = self.read(|v| v.choice.clone()).and_then(|id| migrate_choice(&id, &old, &list));
            self.change(|v| {
                if let Some(m) = migrated {
                    v.choice = Some(m.clone());
                    v.prefs.server = Some(m);
                }
                v.servers = list.clone();
                v.from_account = true;
                v.prefs.routes = Some(list);
            });
            self.save_prefs();
        }
        if can_connect {
            if let Err(code) = self.install_config(known) {
                self.change(|v| v.sync_error = Some(code));
                return;
            }
        } else {
            // No active plan: the engine must not keep an old config of this account.
            self.store_request(None);
        }
        self.change(|v| v.sync_error = None);
    }

    /// A saved route the account no longer offers is reported and the installed config kept: the VPN is
    /// never moved to another route (or to Auto) without the user choosing it.
    fn install_config(&self, routes_known: bool) -> Result<(), String> {
        let chosen = self.read(|v| v.choice.clone());
        if let Some(id) = &chosen {
            if self.read(|v| v.servers.iter().all(|s| &s.id != id)) {
                if routes_known {
                    self.change(|v| v.route_missing = true);
                }
                return Ok(());
            }
        }
        self.change(|v| v.route_missing = false);
        let text = match self.api.vpn_config(chosen.as_deref()) {
            Ok(t) => t,
            Err(e) => {
                self.handle_error(e.clone());
                return if e.code == "signed_out" { Ok(()) } else { Err(e.code) };
            }
        };
        if self.read(|v| v.choice != chosen) {
            return Ok(()); // the user picked another route meanwhile
        }
        let request = EngineRequest::from_api_config(&text).map_err(|_| "config".to_string())?;
        let old = self.stored_request();
        let material_change = old.as_ref().is_none_or(|o| o.material() != request.material());
        if !self.store_request(Some(&request)) {
            return Err("storage_unavailable".into());
        }
        // Only a real change restarts a running VPN; the refreshed file is used at the next connect.
        if material_change && old.is_some() && self.read(|v| v.engine == "connected" && v.wanted) {
            let r = self.helper.replace(request);
            self.apply_status(r);
        }
        Ok(())
    }

    fn handle_error(&self, e: ApiError) {
        if e.code == "signed_out" {
            // The server ended the session. That is not the user's OFF: the switch stays as it is, and the
            // big button, which is still on the home screen when signed out, turns it off.
            return self.sign_out_locally(false);
        }
        // API unreachable: keep showing the last known account; the installed config still connects.
        let snapshot = if self.read(|v| v.account.is_none()) { self.api.offline_snapshot() } else { None };
        self.change(|v| {
            v.sync_error = Some(e.code.clone());
            if let Some((a, s)) = snapshot {
                v.account = Some(a);
                v.subscription = Some(s);
            }
        });
    }

    fn sign_out_locally(&self, user_asked: bool) {
        if user_asked && self.read(|v| v.wanted) {
            let r = self.helper.stop();
            self.apply_status(r);
        }
        self.store_request(None);
        self.change(|v| {
            v.signed_in = false;
            v.account = None;
            v.subscription = None;
            v.servers.clear();
            v.from_account = false;
            v.route_missing = false;
            v.prefs.routes = None;
            if !matches!(v.checkout, CheckoutUi::Failed { .. }) {
                v.checkout = CheckoutUi::Idle;
            }
        });
        self.inner.lock().unwrap().last_request = None;
        self.save_prefs();
    }

    pub fn logout(self: &Arc<Self>) {
        self.background(|c| {
            c.api.logout();
            c.sign_out_locally(true);
        });
    }

    // ---- routes -------------------------------------------------------------------------------------

    /// A route switch is local: the saved config changes and the running engine switches in place. Only a
    /// route the installed config does not have needs the account's config.
    pub fn select_server(self: &Arc<Self>, choice: Option<String>) {
        self.change(|v| {
            v.choice = choice.clone();
            v.prefs.server = choice.clone();
            v.route_missing = false;
        });
        self.save_prefs();
        self.background(move |c| {
            let tag = tag_of(choice.as_deref());
            if let Some(mut request) = c.stored_request() {
                if tag == AUTO_TAG || request.routes.iter().any(|r| r.tag == tag) {
                    request.selected = tag.clone();
                    c.store_request(Some(&request));
                    if c.read(|v| v.engine == "connected") {
                        match c.helper.select(&tag) {
                            Ok(s) => c.apply_status(Ok(s)),
                            Err(_) => {
                                let r = c.helper.replace(request);
                                c.apply_status(r);
                            }
                        }
                    }
                    return;
                }
            }
            if let Err(code) = c.install_config(c.read(|v| v.from_account)) {
                c.change(|v| v.sync_error = Some(code));
            }
        });
    }

    /// Once per opening of the route list, only while connected: each route is tested on its own, twice
    /// before it is called failed. Never restarts or switches the VPN. (RC6 RouteCheck, without its cache.)
    pub fn start_route_check(self: &Arc<Self>) {
        if !self.read(|v| v.engine == "connected") {
            return;
        }
        let epoch = self.route_epoch.fetch_add(1, Ordering::SeqCst) + 1;
        self.change(|v| v.availability.clear());
        self.background(move |c| {
            let ids: Vec<String> = c.read(|v| v.servers.iter().map(|s| s.id.clone()).collect());
            for id in ids {
                if c.route_epoch.load(Ordering::SeqCst) != epoch || c.read(|v| v.engine != "connected") {
                    return;
                }
                let tag = format!("srv-{id}");
                let works = (0..2).any(|_| matches!(c.helper.test_route(&tag), Ok(Some(_))));
                if c.route_epoch.load(Ordering::SeqCst) != epoch {
                    return;
                }
                c.change(|v| {
                    v.availability.insert(id.clone(), if works { "ok" } else { "failed" }.into());
                });
            }
        });
    }

    pub fn stop_route_check(&self) {
        self.route_epoch.fetch_add(1, Ordering::SeqCst);
    }

    /// One check through the tunnel. A single failure only hides the delay; a loss of data is shown after
    /// two failures in a row. Never disconnects, reconnects or switches the route.
    pub fn check_tunnel(&self) {
        let epoch = self.check_epoch.load(Ordering::SeqCst);
        let result = tunnel_check::run();
        self.apply_check(epoch, result, tunnel_check::underlying_network().is_some());
    }

    fn apply_check(&self, epoch: u64, result: CheckResult, online: bool) {
        if epoch != self.check_epoch.load(Ordering::SeqCst) || self.read(|v| v.engine != "connected") {
            return;
        }
        match result {
            CheckResult::Ok(ms) => {
                FAILURES.store(0, Ordering::SeqCst);
                self.change(|v| {
                    v.live_ms = ms;
                    v.traffic = "ok".into();
                });
            }
            CheckResult::Failed { .. } => {
                let failures = FAILURES.fetch_add(1, Ordering::SeqCst) + 1;
                self.change(|v| {
                    v.live_ms = None;
                    if failures >= 2 {
                        v.traffic = if online { "lost" } else { "no_network" }.into();
                    }
                });
            }
        }
    }

    // ---- Telegram -----------------------------------------------------------------------------------

    /// The number comes first; Telegram opens only on the user's button, so the number can be read.
    pub fn telegram_start(self: &Arc<Self>) {
        self.change(|v| v.telegram = TelegramUi::Starting);
        self.background(|c| match c.api.start_telegram_login() {
            Ok(l) => {
                c.change(|v| v.telegram = TelegramUi::Waiting { telegram_url: l.telegram_url.clone(), match_code: l.match_code.clone(), expires_at: l.expires_at });
                c.telegram_wait();
            }
            Err(e) => c.change(|v| v.telegram = TelegramUi::Failed { code: e.code }),
        });
    }

    /// Back on the screen or after a restart: the same request keeps going.
    pub fn telegram_resume(self: &Arc<Self>) {
        if matches!(self.read(|v| v.telegram.clone()), TelegramUi::Waiting { .. }) {
            return;
        }
        let Some(l) = self.api.pending_telegram_login() else { return };
        self.change(|v| v.telegram = TelegramUi::Waiting { telegram_url: l.telegram_url.clone(), match_code: l.match_code.clone(), expires_at: l.expires_at });
        self.background(|c| c.telegram_wait());
    }

    fn telegram_wait(&self) {
        let epoch = self.telegram_epoch.fetch_add(1, Ordering::SeqCst) + 1;
        loop {
            std::thread::sleep(Duration::from_secs(2));
            if self.telegram_epoch.load(Ordering::SeqCst) != epoch {
                return;
            }
            match self.api.poll_telegram_login() {
                Ok(LoginProgress::Pending) => {}
                Ok(LoginProgress::Done) => {
                    self.change(|v| {
                        v.telegram = TelegramUi::Idle;
                        v.signed_in = true;
                    });
                    self.sync_now();
                    return;
                }
                // Offline is not a failure of the request: keep waiting until it expires.
                Err(e) if e.code == "network" || e.status == 0 && e.code != "login_not_found" => {
                    let expired = self.api.pending_telegram_login().is_none_or(|l| l.expires_at <= unix_now());
                    if expired {
                        self.api.cancel_telegram_login();
                        self.change(|v| v.telegram = TelegramUi::Failed { code: "login_expired".into() });
                        return;
                    }
                }
                Err(e) => {
                    self.change(|v| v.telegram = TelegramUi::Failed { code: e.code });
                    return;
                }
            }
        }
    }

    pub fn telegram_cancel(&self) {
        self.telegram_epoch.fetch_add(1, Ordering::SeqCst);
        self.api.cancel_telegram_login();
        self.change(|v| v.telegram = TelegramUi::Idle);
    }

    // ---- email --------------------------------------------------------------------------------------

    pub fn email_send(self: &Arc<Self>, email: String, locale: String) {
        self.change(|v| v.email = EmailUi::Sending);
        self.background(move |c| {
            let next = match c.api.start_email_login(email.trim(), &locale) {
                Ok(()) => EmailUi::EnterCode { email: email.trim().to_string(), sent_at: unix_now(), busy: false, error: None },
                Err(e) => EmailUi::Failed { code: e.code },
            };
            c.change(|v| v.email = next);
        });
    }

    pub fn email_verify(self: &Arc<Self>, code: String) {
        let EmailUi::EnterCode { email, sent_at, busy: false, .. } = self.read(|v| v.email.clone()) else { return };
        self.change(|v| v.email = EmailUi::EnterCode { email: email.clone(), sent_at, busy: true, error: None });
        self.background(move |c| match c.api.verify_email_code(&email, code.trim()) {
            Ok(()) => {
                c.change(|v| {
                    v.email = EmailUi::EnterEmail;
                    v.signed_in = true;
                });
                c.sync_now();
            }
            Err(e) => c.change(|v| v.email = EmailUi::EnterCode { email, sent_at, busy: false, error: Some(e.code) }),
        });
    }

    /// One request per tap, at most once a minute; never repeated in the background.
    pub fn email_resend(self: &Arc<Self>, locale: String) {
        let EmailUi::EnterCode { email, sent_at, busy: false, .. } = self.read(|v| v.email.clone()) else { return };
        if unix_now() - sent_at < EMAIL_RESEND_SECS {
            return;
        }
        self.change(|v| v.email = EmailUi::EnterCode { email: email.clone(), sent_at, busy: true, error: None });
        self.background(move |c| {
            let next = match c.api.start_email_login(&email, &locale) {
                Ok(()) => EmailUi::EnterCode { email, sent_at: unix_now(), busy: false, error: None },
                Err(e) => EmailUi::EnterCode { email, sent_at, busy: false, error: Some(e.code) },
            };
            c.change(|v| v.email = next);
        });
    }

    pub fn email_reset(&self) {
        self.change(|v| v.email = EmailUi::EnterEmail);
    }

    // ---- payment ------------------------------------------------------------------------------------

    /// Returns the one-use link for the shell to open; it is not stored in the view or anywhere else.
    /// Nothing else is ever opened instead: on any error the user is told what to do.
    pub fn checkout_start(&self, plan: &str, action: &str, profile: Option<String>, language: &str, channel: Option<String>) -> Option<String> {
        if matches!(self.read(|v| v.checkout.clone()), CheckoutUi::Requesting) {
            return None;
        }
        self.change(|v| v.checkout = CheckoutUi::Requesting);
        let locale = checkout::api_locale(language);
        let (source, has_profiles) = self.read(|v| {
            let a = v.account.as_ref();
            let profiles = v.subscription.as_ref().map_or(0, |s| s.profiles.len());
            let source = profile.as_ref().or(v.subscription.as_ref().and_then(|s| s.effective_id.as_ref())).and_then(|id| {
                a.and_then(|x| x.vpn_sources.iter().find(|(i, _)| i == id).map(|(_, s)| s.clone()))
            });
            (source, profiles > 0)
        });
        self.inner.lock().unwrap().last_request = Some((plan.to_string(), action.to_string(), profile.clone(), language.to_string()));
        match self.api.create_checkout_intent(plan, action, profile.as_deref(), locale, channel.as_deref()) {
            Ok(i) => {
                let pid = profile.unwrap_or_default();
                self.change(|v| v.checkout = CheckoutUi::Opened { channel: i.channel.clone(), profile_id: pid, operation_id: i.id.clone(), order: "awaiting_checkout".into(), checking: false });
                Some(i.url)
            }
            Err(e) => {
                let problem = checkout::problem(&e.code, e.status).to_string();
                let action = action.to_string();
                self.change(|v| v.checkout = CheckoutUi::Failed { problem, action, source, has_profiles });
                None
            }
        }
    }

    /// The result is read from the App API only. Returning from a browser proves nothing.
    fn check_payment(&self) {
        let CheckoutUi::Opened { channel, profile_id, operation_id, order, .. } = self.read(|v| v.checkout.clone()) else { return };
        match self.api.checkout_status(Some(&operation_id)) {
            Ok(Some(r)) if r.state == "paid" => {
                let _ = (&channel, &profile_id);
                self.change(|v| v.checkout = CheckoutUi::Confirmed);
            }
            Ok(Some(r)) if r.state != order => {
                self.change(|v| {
                    if let CheckoutUi::Opened { order, .. } = &mut v.checkout {
                        *order = r.state.clone();
                    }
                });
            }
            _ => {}
        }
    }

    pub fn checkout_check(self: &Arc<Self>) {
        self.change(|v| {
            if let CheckoutUi::Opened { checking, .. } = &mut v.checkout {
                *checking = true;
            }
        });
        self.background(|c| {
            c.sync_now();
            c.change(|v| {
                if let CheckoutUi::Opened { checking, .. } = &mut v.checkout {
                    *checking = false;
                }
            });
        });
    }

    pub fn checkout_reset(&self) {
        self.change(|v| v.checkout = CheckoutUi::Idle);
    }

    /// Same plan, new one-use link.
    pub fn checkout_retry(&self) -> Option<String> {
        let (plan, action, profile, language) = self.inner.lock().unwrap().last_request.clone()?;
        self.checkout_start(&plan, &action, profile, &language, None)
    }

    // ---- email link ---------------------------------------------------------------------------------

    pub fn email_link_start(&self, email: &str, locale: &str) -> Value {
        result_json(self.api.start_email_link(email, locale).map(|()| json!({})))
    }

    pub fn email_link_confirm(self: &Arc<Self>, email: &str, code: &str) -> Value {
        let r = self.api.confirm_email_link(email, code);
        if r.is_ok() {
            self.sync();
        }
        result_json(r.map(|_| json!({})))
    }

    pub fn email_unlink(self: &Arc<Self>, id: &str) -> Value {
        let r = self.api.unlink_email(id);
        if r.is_ok() {
            self.sync();
        }
        result_json(r.map(|()| json!({})))
    }

    // ---- preferences --------------------------------------------------------------------------------

    pub fn mark_welcome_seen(&self) {
        self.change(|v| v.prefs.welcome_seen = true);
        self.save_prefs();
    }

    pub fn set_theme(&self, theme: &str) {
        if !matches!(theme, "system" | "light" | "dark") {
            return;
        }
        self.change(|v| v.prefs.theme = theme.to_string());
        self.save_prefs();
    }

    pub fn set_language(&self, language: &str) {
        if !matches!(language, "system" | "ru" | "en" | "fa" | "zh-CN") {
            return;
        }
        self.change(|v| v.prefs.language = language.to_string());
        self.save_prefs();
    }
}

static FAILURES: AtomicU64 = AtomicU64::new(0);

fn result_json(r: Result<Value, ApiError>) -> Value {
    match r {
        Ok(v) => json!({"ok": true, "value": v}),
        Err(e) => json!({"ok": false, "code": e.code, "status": e.status}),
    }
}

#[cfg(test)]
mod version_tests {
    use super::versions_differ;
    use marz_engine::ipc::{Status, PROTOCOL};

    #[test]
    fn a_helper_of_another_version_is_a_damaged_install_on_windows() {
        assert!(!versions_differ(PROTOCOL, "0.1.6", "0.1.6", true));
        assert!(versions_differ(PROTOCOL, "0.1.5", "0.1.6", true));
        assert!(versions_differ(PROTOCOL, "", "0.1.6", true), "a helper older than the field");
        assert!(versions_differ(PROTOCOL + 1, "0.1.6", "0.1.6", true));
    }

    #[test]
    fn a_linux_package_may_keep_an_earlier_helper_of_the_same_protocol() {
        assert!(!versions_differ(PROTOCOL, "", "0.1.6", false));
        assert!(versions_differ(PROTOCOL - 1, "", "0.1.6", false));
    }

    #[test]
    fn the_status_of_an_older_helper_still_parses() {
        let old = r#"{"protocol":4,"state":"stopped","wanted":false,"selected":null,"active":null,"error":null,"restarts":0,"kill_switch":"off","needs_request":false,"session":"none"}"#;
        let s: Status = serde_json::from_str(old).unwrap();
        assert_eq!(s.version, "");
    }
}
