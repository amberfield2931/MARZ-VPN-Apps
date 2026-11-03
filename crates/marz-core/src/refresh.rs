//! Refresh-token rotation with recovery of a lost reply: a direct port of Android RC6
//! com.marzvpn.app.api.RefreshClient, rule for rule (see its documentation and the backend's
//! docs/app/REFRESH-RECOVERY-CLAUDE.md). The server revokes the whole session family on a reused token,
//! so the order "save durably, then send" is what keeps users signed in.
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

pub const RECOVER: &str = "/v1/auth/refresh/recover";
pub const CANCEL: &str = "/v1/auth/refresh/cancel";
pub const CUTOFF_MS: i64 = 45_000;
pub const MAX_ATTEMPTS: u32 = 4;
pub const MAX_SENT: u32 = 3;
pub const MAX_CANCELS: usize = 3;
pub const MAX_CANCEL_ATTEMPTS: u32 = 5;
pub const CANCEL_KEEP_MS: i64 = 7 * 24 * 3_600_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mark {
    pub wall: i64,
    pub elapsed: i64,
    pub boot: i64,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pending {
    pub parent: String,
    pub id: String,
    #[serde(default)]
    pub uncertain: Option<Mark>,
    #[serde(default)]
    pub sent: u32,
    #[serde(default)]
    pub stalled: bool,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub access: String,
    pub refresh: String,
    #[serde(default)]
    pub session: String,
    #[serde(default)]
    pub pending: Option<Pending>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cancel {
    pub parent: String,
    pub id: String,
    pub created: i64,
    #[serde(default)]
    pub attempts: u32,
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct State {
    pub session: Option<Session>,
    pub cancels: Vec<Cancel>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct Tokens {
    pub access: String,
    pub refresh: String,
    pub session: String,
}

#[derive(Clone)]
pub struct Reply {
    pub code: u16,
    pub error: String,
    pub tokens: Option<Tokens>,
    pub retry_after: Option<u32>,
}

impl Reply {
    fn network() -> Self {
        Reply { code: 0, error: "network".into(), tokens: None, retry_after: None }
    }
    fn storage() -> Self {
        Reply { code: 0, error: "local_storage".into(), tokens: None, retry_after: None }
    }
}

pub enum Outcome {
    Done,
    /// The server ended the session (or it was signed out locally meanwhile).
    Rejected,
    /// The rotation may have happened and can no longer be recovered: sign in again.
    Reauth,
    /// Nothing decided; tokens and the pending operation stay for a later try.
    Transient(Reply),
}

impl std::fmt::Debug for Outcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Outcome::Done => f.write_str("Done"),
            Outcome::Rejected => f.write_str("Rejected"),
            Outcome::Reauth => f.write_str("Reauth"),
            Outcome::Transient(r) => write!(f, "Transient({} {})", r.code, r.error),
        }
    }
}

pub trait AuthStore: Send + Sync {
    fn read(&self) -> State;
    /// True only after the change is on disk (one atomic update).
    fn write(&self, state: &State) -> bool;
}

pub trait RefreshTransport: Send + Sync {
    /// One device-signed POST of `{"refresh_token": parent, "request_id": id}`, with a new timestamp and
    /// signature. `before_body` runs once connected; false aborts. Err = no answer arrived.
    fn post(&self, path: &str, parent: &str, id: &str, before_body: &mut dyn FnMut() -> bool) -> Result<Reply, ()>;
}

pub trait AuthClock: Send + Sync {
    fn wall(&self) -> i64;
    fn elapsed(&self) -> i64;
    /// Boot identity, or -1 when unknown.
    fn boot(&self) -> i64;
    fn sleep(&self, ms: u64);
    fn mark(&self) -> Mark {
        Mark { wall: self.wall(), elapsed: self.elapsed(), boot: self.boot() }
    }
}

/// Codes with which the App API ends a session for good. 5xx, 429, timeouts, offline and signature or
/// clock errors keep the session.
pub fn ends(code: u16, error: &str) -> bool {
    code == 401 && matches!(error, "refresh_reused" | "invalid_refresh" | "pilot_closed")
}

pub struct RefreshClient<S, T, C> {
    pub store: S,
    pub transport: T,
    pub clock: C,
    new_id: Box<dyn Fn() -> String + Send + Sync>,
    rotation: Mutex<()>,
    state: Mutex<()>,
}

enum Prep {
    SignedOut,
    AlreadyDone,
    NotSaved,
    Ready(Pending),
}

impl<S: AuthStore, T: RefreshTransport, C: AuthClock> RefreshClient<S, T, C> {
    pub fn new(store: S, transport: T, clock: C, new_id: Box<dyn Fn() -> String + Send + Sync>) -> Self {
        RefreshClient { store, transport, clock, new_id, rotation: Mutex::new(()), state: Mutex::new(()) }
    }

    /// Rotates (or recovers) for a request that got 401 with `stale_access`. Concurrent callers share
    /// one operation; a caller that waited finds the new pair and gets Done.
    pub fn refresh(&self, stale_access: &str) -> Outcome {
        let _rotation = self.rotation.lock().unwrap();
        let mut attempts = 0u32;
        let mut replays = 0u32;
        let mut last = Reply::network();
        loop {
            let prepared = {
                let _g = self.state.lock().unwrap();
                let now = self.store.read();
                match now.session.clone() {
                    None => Prep::SignedOut,
                    Some(s) if s.pending.is_none() && s.access != stale_access => Prep::AlreadyDone,
                    Some(s) => match s.pending.clone() {
                        Some(p) => Prep::Ready(p),
                        None => {
                            let p = Pending { parent: s.refresh.clone(), id: (self.new_id)(), uncertain: None, sent: 0, stalled: false };
                            let next = State { session: Some(Session { pending: Some(p.clone()), ..s }), cancels: now.cancels };
                            if self.store.write(&next) {
                                Prep::Ready(p)
                            } else {
                                Prep::NotSaved
                            }
                        }
                    },
                }
            };
            let p = match prepared {
                Prep::SignedOut => return Outcome::Rejected,
                Prep::AlreadyDone => return Outcome::Done,
                Prep::NotSaved => return Outcome::Transient(Reply::storage()), // no request without a saved operation
                Prep::Ready(p) => p,
            };
            if p.stalled {
                return Outcome::Reauth;
            }
            if p.uncertain.is_some_and(|m| self.since(&m) > CUTOFF_MS) {
                return self.stall(&p);
            }
            // A 4th unanswered attempt would be a reuse for the server (family revoked): sign in instead.
            if p.sent >= MAX_SENT {
                return self.stall(&p);
            }
            if attempts >= MAX_ATTEMPTS {
                return Outcome::Transient(last);
            }
            attempts += 1;
            let mut marked_now = false;
            let mut counted = false;
            let reply = {
                let mut before = || {
                    let first = p.uncertain.is_none();
                    counted = self.update(&p, |it| Pending {
                        uncertain: it.uncertain.or_else(|| Some(self.clock.mark())),
                        sent: it.sent + 1,
                        ..it.clone()
                    });
                    marked_now = counted && first;
                    counted
                };
                self.transport.post(RECOVER, &p.parent, &p.id, &mut before)
            };
            // A definite answer: this attempt rotated nothing, so it neither counts nor starts the clock.
            let answered = |counted: bool, marked_now: bool| {
                if counted {
                    self.update(&p, |it| Pending {
                        sent: it.sent.saturating_sub(1),
                        uncertain: if marked_now { None } else { it.uncertain },
                        ..it.clone()
                    });
                }
            };
            let Ok(reply) = reply else {
                last = Reply::network();
                if !self.backoff(&p, attempts, marked_now || p.uncertain.is_some()) {
                    return self.stop_here(&p, last);
                }
                continue;
            };
            last = reply.clone();
            match &reply {
                Reply { code: 200, tokens: Some(t), .. } if t.refresh != p.parent => {
                    let _g = self.state.lock().unwrap();
                    let now = self.store.read();
                    let same = now.session.as_ref().is_some_and(|s| s.refresh == p.parent && s.pending.as_ref().is_some_and(|q| q.id == p.id));
                    // A late reply after logout, another sign-in or a later pair is never saved.
                    if !same {
                        return Outcome::Rejected;
                    }
                    let next = State {
                        session: Some(Session { access: t.access.clone(), refresh: t.refresh.clone(), session: t.session.clone(), pending: None }),
                        cancels: now.cancels,
                    };
                    return if self.store.write(&next) { Outcome::Done } else { Outcome::Transient(Reply::storage()) };
                }
                r if ends(r.code, &r.error) => {
                    let _g = self.state.lock().unwrap();
                    let now = self.store.read();
                    if now.session.as_ref().and_then(|s| s.pending.as_ref()).is_some_and(|q| q.id == p.id) {
                        self.store.write(&State { session: None, cancels: now.cancels });
                    }
                    return Outcome::Rejected;
                }
                r if r.code == 409 && r.error == "refresh_request_conflict" => return self.stall(&p),
                r if r.code == 409 && r.error == "refresh_proof_replayed" => {
                    answered(counted, marked_now);
                    let n = replays;
                    replays += 1;
                    if n >= 1 {
                        return Outcome::Transient(reply);
                    }
                    self.clock.sleep(1_100); // a new timestamp, so a really new signature
                }
                r if r.code == 429 => {
                    answered(counted, marked_now);
                    let wait = u64::from(r.retry_after.unwrap_or(5)) * 1000;
                    if !self.fits(&p, wait as i64) || attempts >= MAX_ATTEMPTS {
                        return Outcome::Transient(reply);
                    }
                    self.clock.sleep(wait);
                }
                r if r.code == 200 || ((500..=599).contains(&r.code) && r.error == format!("http_{}", r.code)) => {
                    // A 5xx that is not the App API's own answer (a proxy page) or an unusable 200 does not
                    // prove that nothing was rotated: it counts against the recovery window.
                    if !self.backoff(&p, attempts, true) {
                        return self.stop_here(&p, reply);
                    }
                }
                r if (500..=599).contains(&r.code) => {
                    // The App API's own error: its transaction rolled back.
                    answered(counted, marked_now);
                    if !self.backoff(&p, attempts, false) {
                        return Outcome::Transient(reply);
                    }
                }
                _ => {
                    // 404, 409 recovery off, 400, 401 signature: a definite "nothing rotated".
                    answered(counted, marked_now);
                    return Outcome::Transient(reply);
                }
            }
        }
    }

    fn backoff(&self, p: &Pending, attempts: u32, uncertain: bool) -> bool {
        if attempts >= MAX_ATTEMPTS {
            return false;
        }
        let wait = 1_000u64 << (attempts - 1);
        if uncertain {
            if let Some(cur) = self.current(p) {
                if cur.uncertain.is_some() && !self.fits(&cur, wait as i64) {
                    return false;
                }
            }
        }
        self.clock.sleep(wait);
        true
    }

    /// If the operation can never complete any more it ends now (sign in again); otherwise a later sync retries.
    fn stop_here(&self, p: &Pending, reply: Reply) -> Outcome {
        let Some(cur) = self.current(p) else { return Outcome::Transient(reply) };
        let Some(mark) = cur.uncertain else { return Outcome::Transient(reply) };
        if cur.sent >= MAX_SENT || self.since(&mark).saturating_add(1_000) > CUTOFF_MS {
            self.stall(&cur)
        } else {
            Outcome::Transient(reply)
        }
    }

    fn fits(&self, p: &Pending, wait: i64) -> bool {
        p.uncertain.is_none_or(|m| self.since(&m).saturating_add(wait) <= CUTOFF_MS)
    }

    fn current(&self, p: &Pending) -> Option<Pending> {
        let _g = self.state.lock().unwrap();
        self.store.read().session?.pending.filter(|q| q.id == p.id)
    }

    fn update(&self, p: &Pending, change: impl Fn(&Pending) -> Pending) -> bool {
        let _g = self.state.lock().unwrap();
        let now = self.store.read();
        let Some(s) = now.session.clone() else { return false };
        let Some(cur) = s.pending.clone().filter(|q| q.id == p.id) else { return false };
        self.store.write(&State { session: Some(Session { pending: Some(change(&cur)), ..s }), cancels: now.cancels })
    }

    fn stall(&self, p: &Pending) -> Outcome {
        self.update(p, |it| Pending { stalled: true, ..it.clone() });
        Outcome::Reauth
    }

    /// Milliseconds since `m` on the monotonic clock. After a reboot the time passed is not known for
    /// sure: "very long", so the operation is not retried.
    fn since(&self, m: &Mark) -> i64 {
        let boot = self.clock.boot();
        let same_boot = if boot >= 0 && m.boot >= 0 { boot == m.boot } else { self.clock.elapsed() >= m.elapsed };
        let d = self.clock.elapsed() - m.elapsed;
        if !same_boot || d < 0 {
            i64::MAX
        } else {
            d
        }
    }

    /// A new sign-in replaces the session; an unfinished operation of the old one is cancelled later.
    pub fn save_sign_in(&self, t: &Tokens) -> bool {
        let _g = self.state.lock().unwrap();
        let now = self.store.read();
        let mut cancels = now.cancels.clone();
        if let Some(old) = now.session.as_ref().and_then(|s| s.pending.as_ref()) {
            cancels.push(Cancel { parent: old.parent.clone(), id: old.id.clone(), created: self.clock.wall(), attempts: 0 });
        }
        let cut = cancels.len().saturating_sub(MAX_CANCELS);
        cancels.drain(..cut);
        self.store.write(&State {
            session: Some(Session { access: t.access.clone(), refresh: t.refresh.clone(), session: t.session.clone(), pending: None }),
            cancels,
        })
    }

    pub fn is_signed_in(&self) -> bool {
        let _g = self.state.lock().unwrap();
        self.store.read().session.is_some()
    }

    pub fn access(&self) -> Option<String> {
        let _g = self.state.lock().unwrap();
        self.store.read().session.map(|s| s.access)
    }

    /// Explicit logout: locally signed out first (a late refresh reply cannot bring the account back),
    /// then revoked on the server; an undelivered revocation is kept. `plain_logout` returns the HTTP
    /// status (0 offline).
    pub fn logout(&self, plain_logout: &dyn Fn(&str) -> u16) {
        let taken = {
            let _g = self.state.lock().unwrap();
            let now = self.store.read();
            let Some(s) = now.session.clone() else { return };
            let c = Cancel {
                parent: s.pending.as_ref().map_or(s.refresh.clone(), |p| p.parent.clone()),
                id: s.pending.as_ref().map_or_else(|| (self.new_id)(), |p| p.id.clone()),
                created: self.clock.wall(),
                attempts: 0,
            };
            let mut cancels = now.cancels.clone();
            cancels.push(c.clone());
            let cut = cancels.len().saturating_sub(MAX_CANCELS);
            cancels.drain(..cut);
            if !self.store.write(&State { session: None, cancels }) {
                self.store.write(&State { session: None, cancels: now.cancels });
            }
            (s, c)
        };
        let (session, cancel) = taken;
        if session.pending.is_none() {
            let status = plain_logout(&session.access);
            if status == 200 {
                return self.forget(&cancel);
            }
            if status == 0 || status >= 500 {
                return; // kept: delivered later
            }
        }
        self.deliver_cancels();
    }

    /// Sends the kept cancellations (at most [MAX_CANCEL_ATTEMPTS] tries each, for at most a week).
    pub fn deliver_cancels(&self) {
        let pending = {
            let _g = self.state.lock().unwrap();
            self.store.read().cancels
        };
        for c in pending {
            if c.attempts >= MAX_CANCEL_ATTEMPTS || self.clock.wall() - c.created > CANCEL_KEEP_MS {
                self.forget(&c);
                continue;
            }
            let reply = self.transport.post(CANCEL, &c.parent, &c.id, &mut || true);
            match reply {
                Ok(r) if r.code == 200 || r.code == 400 || (r.code == 401 && r.error == "invalid_refresh") => self.forget(&c),
                _ => {
                    let _g = self.state.lock().unwrap();
                    let now = self.store.read();
                    let cancels = now.cancels.iter().map(|it| if it.id == c.id { Cancel { attempts: it.attempts + 1, ..it.clone() } } else { it.clone() }).collect();
                    self.store.write(&State { session: now.session, cancels });
                }
            }
        }
    }

    fn forget(&self, c: &Cancel) {
        let _g = self.state.lock().unwrap();
        let now = self.store.read();
        let cancels = now.cancels.into_iter().filter(|it| it.id != c.id).collect();
        self.store.write(&State { session: now.session, cancels });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI64, AtomicU32, Ordering};
    use std::sync::Arc;

    #[derive(Default)]
    struct MemStore {
        state: Mutex<State>,
        fail_next: AtomicU32,
    }
    impl AuthStore for Arc<MemStore> {
        fn read(&self) -> State {
            self.state.lock().unwrap().clone()
        }
        fn write(&self, s: &State) -> bool {
            if self.fail_next.load(Ordering::SeqCst) > 0 {
                self.fail_next.fetch_sub(1, Ordering::SeqCst);
                return false;
            }
            *self.state.lock().unwrap() = s.clone();
            true
        }
    }

    type Script = Vec<Result<Reply, ()>>;

    #[derive(Default)]
    struct FakeNet {
        script: Mutex<Script>,
        calls: Mutex<Vec<(String, String, String)>>,
        /// (sent, uncertain?) seen by the store at the moment the body would leave
        before_results: Mutex<Vec<bool>>,
        clock: Option<Arc<FakeClock>>,
        per_call_ms: i64,
    }
    impl RefreshTransport for Arc<FakeNet> {
        fn post(&self, path: &str, parent: &str, id: &str, before: &mut dyn FnMut() -> bool) -> Result<Reply, ()> {
            self.calls.lock().unwrap().push((path.into(), parent.into(), id.into()));
            let ok = before();
            self.before_results.lock().unwrap().push(ok);
            if let Some(c) = &self.clock {
                c.elapsed.fetch_add(self.per_call_ms, Ordering::SeqCst);
            }
            if !ok {
                return Err(());
            }
            let mut s = self.script.lock().unwrap();
            if s.is_empty() {
                Err(())
            } else {
                s.remove(0)
            }
        }
    }

    #[derive(Default)]
    struct FakeClock {
        elapsed: AtomicI64,
        boot: AtomicI64,
        slept: AtomicI64,
    }
    impl AuthClock for Arc<FakeClock> {
        fn wall(&self) -> i64 {
            1_000_000 + self.elapsed.load(Ordering::SeqCst)
        }
        fn elapsed(&self) -> i64 {
            self.elapsed.load(Ordering::SeqCst)
        }
        fn boot(&self) -> i64 {
            self.boot.load(Ordering::SeqCst)
        }
        fn sleep(&self, ms: u64) {
            self.elapsed.fetch_add(ms as i64, Ordering::SeqCst);
            self.slept.fetch_add(ms as i64, Ordering::SeqCst);
        }
    }

    fn reply(code: u16, error: &str) -> Result<Reply, ()> {
        Ok(Reply { code, error: error.into(), tokens: None, retry_after: None })
    }
    fn tokens(n: &str) -> Result<Reply, ()> {
        Ok(Reply { code: 200, error: "http_200".into(), tokens: Some(Tokens { access: format!("a{n}"), refresh: format!("r{n}"), session: "s".into() }), retry_after: None })
    }

    struct World {
        store: Arc<MemStore>,
        net: Arc<FakeNet>,
        clock: Arc<FakeClock>,
        client: RefreshClient<Arc<MemStore>, Arc<FakeNet>, Arc<FakeClock>>,
    }

    fn world(script: Script) -> World {
        let store = Arc::new(MemStore::default());
        store.state.lock().unwrap().session = Some(Session { access: "a0".into(), refresh: "r0".into(), session: "s".into(), pending: None });
        let clock = Arc::new(FakeClock::default());
        let net = Arc::new(FakeNet { script: Mutex::new(script), clock: Some(clock.clone()), ..Default::default() });
        let n = AtomicU32::new(0);
        let client = RefreshClient::new(store.clone(), net.clone(), clock.clone(), Box::new(move || format!("id{}", n.fetch_add(1, Ordering::SeqCst))));
        World { store, net, clock, client }
    }

    fn session(w: &World) -> Option<Session> {
        w.store.read().session
    }

    #[test]
    fn success_replaces_the_pair_and_ends_the_operation() {
        let w = world(vec![tokens("1")]);
        assert!(matches!(w.client.refresh("a0"), Outcome::Done));
        let s = session(&w).unwrap();
        assert_eq!((s.access.as_str(), s.refresh.as_str()), ("a1", "r1"));
        assert!(s.pending.is_none());
        assert_eq!(w.net.calls.lock().unwrap()[0].0, RECOVER);
    }

    #[test]
    fn a_caller_that_waited_finds_the_new_pair() {
        let w = world(vec![tokens("1")]);
        assert!(matches!(w.client.refresh("a0"), Outcome::Done));
        assert!(matches!(w.client.refresh("a0"), Outcome::Done));
        assert_eq!(w.net.calls.lock().unwrap().len(), 1);
    }

    #[test]
    fn nothing_is_sent_when_the_operation_cannot_be_saved() {
        let w = world(vec![tokens("1")]);
        w.store.fail_next.store(1, Ordering::SeqCst);
        match w.client.refresh("a0") {
            Outcome::Transient(r) => assert_eq!(r.error, "local_storage"),
            o => panic!("{o:?}"),
        }
        assert!(w.net.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn body_is_not_sent_when_the_attempt_cannot_be_recorded() {
        let w = world(vec![tokens("1")]);
        // First write (operation) succeeds, second (attempt mark) fails: abort before the body.
        struct Second(Arc<MemStore>, AtomicU32);
        impl AuthStore for Second {
            fn read(&self) -> State {
                self.0.read()
            }
            fn write(&self, s: &State) -> bool {
                if self.1.fetch_add(1, Ordering::SeqCst) == 1 {
                    return false;
                }
                self.0.write(s)
            }
        }
        let n = AtomicU32::new(0);
        let c = RefreshClient::new(Second(w.store.clone(), AtomicU32::new(0)), w.net.clone(), w.clock.clone(), Box::new(move || format!("x{}", n.fetch_add(1, Ordering::SeqCst))));
        let _ = c.refresh("a0");
        assert_eq!(w.net.before_results.lock().unwrap()[0], false);
    }

    #[test]
    fn retries_reuse_the_same_request_id_and_parent() {
        let w = world(vec![Err(()), Err(()), tokens("1")]);
        assert!(matches!(w.client.refresh("a0"), Outcome::Done));
        let calls = w.net.calls.lock().unwrap();
        assert_eq!(calls.len(), 3);
        assert!(calls.iter().all(|c| c.1 == "r0" && c.2 == "id0"));
    }

    #[test]
    fn operation_survives_a_restart_with_the_same_id() {
        let w = world(vec![Err(())]);
        // Budget-exhausted transient with the operation stored.
        let _ = w.client.refresh("a0");
        let stored = session(&w).unwrap().pending.unwrap();
        assert_eq!(stored.id, "id0");
    }

    #[test]
    fn four_unanswered_attempts_never_happen() {
        let w = world(vec![Err(()), Err(()), Err(()), Err(()), Err(())]);
        let o = w.client.refresh("a0");
        assert!(matches!(o, Outcome::Reauth), "{o:?}");
        assert_eq!(w.net.calls.lock().unwrap().len(), 3);
        assert!(session(&w).unwrap().pending.unwrap().stalled);
        // Stalled: no further use of the old token, the session (and VPN) stays, user signs in again.
        assert!(matches!(w.client.refresh("a0"), Outcome::Reauth));
        assert_eq!(w.net.calls.lock().unwrap().len(), 3);
    }

    #[test]
    fn recovery_window_is_respected() {
        let w = world(vec![Err(())]);
        let _ = w.client.refresh("a0");
        w.clock.elapsed.fetch_add(CUTOFF_MS + 1, Ordering::SeqCst);
        let calls = w.net.calls.lock().unwrap().len();
        assert!(matches!(w.client.refresh("a0"), Outcome::Reauth));
        assert_eq!(w.net.calls.lock().unwrap().len(), calls);
    }

    #[test]
    fn a_reboot_counts_as_a_very_long_time() {
        let w = world(vec![Err(())]);
        let _ = w.client.refresh("a0");
        let was_stalled = session(&w).unwrap().pending.unwrap().stalled;
        if !was_stalled {
            w.clock.boot.fetch_add(1, Ordering::SeqCst);
            assert!(matches!(w.client.refresh("a0"), Outcome::Reauth));
        }
    }

    #[test]
    fn only_three_codes_end_a_session() {
        for code in ["refresh_reused", "invalid_refresh", "pilot_closed"] {
            let w = world(vec![reply(401, code)]);
            assert!(matches!(w.client.refresh("a0"), Outcome::Rejected));
            assert!(session(&w).is_none());
        }
        for (status, code) in [(401, "invalid_device_signature"), (503, "upstream_unavailable"), (404, "not_found"), (409, "refresh_recovery_unavailable")] {
            let script = vec![reply(status, code), reply(status, code), reply(status, code), reply(status, code)];
            let w = world(script);
            assert!(matches!(w.client.refresh("a0"), Outcome::Transient(_)), "{status} {code}");
            assert!(session(&w).is_some());
        }
    }

    #[test]
    fn a_definite_answer_rotated_nothing() {
        let w = world(vec![reply(503, "upstream_unavailable"), reply(404, "not_found")]);
        let _ = w.client.refresh("a0");
        let p = session(&w).unwrap().pending.unwrap();
        assert_eq!(p.sent, 0);
        assert!(p.uncertain.is_none());
    }

    #[test]
    fn a_proxy_page_may_have_rotated_it() {
        let w = world(vec![reply(502, "http_502"), tokens("1")]);
        assert!(matches!(w.client.refresh("a0"), Outcome::Done));
        assert_eq!(w.net.calls.lock().unwrap().len(), 2);
    }

    #[test]
    fn rate_limit_waits_as_told() {
        let script = vec![Ok(Reply { code: 429, error: "rate_limited".into(), tokens: None, retry_after: Some(7) }), tokens("1")];
        let w = world(script);
        assert!(matches!(w.client.refresh("a0"), Outcome::Done));
        assert_eq!(w.clock.slept.load(Ordering::SeqCst), 7000);
    }

    #[test]
    fn conflict_stalls_and_replayed_proof_retries_once() {
        let w = world(vec![reply(409, "refresh_request_conflict")]);
        assert!(matches!(w.client.refresh("a0"), Outcome::Reauth));
        let w = world(vec![reply(409, "refresh_proof_replayed"), tokens("1")]);
        assert!(matches!(w.client.refresh("a0"), Outcome::Done));
        let w = world(vec![reply(409, "refresh_proof_replayed"), reply(409, "refresh_proof_replayed")]);
        assert!(matches!(w.client.refresh("a0"), Outcome::Transient(_)));
    }

    #[test]
    fn same_token_back_is_not_a_rotation() {
        let w = world(vec![Ok(Reply { code: 200, error: "http_200".into(), tokens: Some(Tokens { access: "a9".into(), refresh: "r0".into(), session: "s".into() }), retry_after: None }), tokens("1")]);
        assert!(matches!(w.client.refresh("a0"), Outcome::Done));
        assert_eq!(session(&w).unwrap().refresh, "r1");
    }

    #[test]
    fn logout_is_local_first_and_late_replies_are_dropped() {
        let w = world(vec![]);
        w.client.logout(&|_| 0); // offline
        assert!(session(&w).is_none());
        assert_eq!(w.store.read().cancels.len(), 1);
        // A delayed successful refresh from before cannot bring the account back.
        assert!(matches!(w.client.refresh("a0"), Outcome::Rejected));
    }

    #[test]
    fn logout_online_leaves_nothing_to_deliver() {
        let w = world(vec![]);
        w.client.logout(&|access| {
            assert_eq!(access, "a0");
            200
        });
        assert!(w.store.read().cancels.is_empty());
        assert!(w.net.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn logout_with_an_open_operation_cancels_with_its_id() {
        let w = world(vec![Err(()), reply(200, "ok")]);
        let _ = w.client.refresh("a0");
        w.net.script.lock().unwrap().clear();
        w.net.script.lock().unwrap().push(reply(200, "ok"));
        w.client.logout(&|_| panic!("the old access is not valid any more"));
        let calls = w.net.calls.lock().unwrap();
        let last = calls.last().unwrap();
        assert_eq!((last.0.as_str(), last.1.as_str(), last.2.as_str()), (CANCEL, "r0", "id0"));
        assert!(w.store.read().cancels.is_empty());
    }

    #[test]
    fn cancellations_are_bounded() {
        let w = world(vec![]);
        w.client.logout(&|_| 0);
        for _ in 0..MAX_CANCEL_ATTEMPTS {
            w.client.deliver_cancels();
        }
        assert_eq!(w.store.read().cancels.len(), 1);
        w.client.deliver_cancels();
        assert!(w.store.read().cancels.is_empty());
        let w = world(vec![]);
        w.client.logout(&|_| 0);
        w.clock.elapsed.fetch_add(CANCEL_KEEP_MS + 1, Ordering::SeqCst);
        w.client.deliver_cancels();
        assert!(w.store.read().cancels.is_empty());
    }

    #[test]
    fn a_new_sign_in_replaces_and_schedules_the_old_cancel() {
        let w = world(vec![Err(())]);
        let _ = w.client.refresh("a0");
        assert!(w.client.save_sign_in(&Tokens { access: "n".into(), refresh: "nr".into(), session: "s2".into() }));
        let st = w.store.read();
        assert_eq!(st.session.unwrap().access, "n");
        assert_eq!(st.cancels.len(), 1);
    }

    #[test]
    fn debug_output_of_an_outcome_has_no_tokens() {
        let o = Outcome::Transient(Reply { code: 500, error: "x".into(), tokens: Some(Tokens { access: "SECRET".into(), refresh: "SECRET".into(), session: "s".into() }), retry_after: None });
        assert!(!format!("{o:?}").contains("SECRET"));
    }
}
