// Preview only (tests/ui/shots.sh): synthetic states for screenshots, never part of the app package.
const q = new URLSearchParams(location.search), scenario = q.get('s') || 'home_off', language = q.get('lang') || 'ru';
const day = 86400, now = Math.floor(Date.now() / 1000);
const servers = [
  { id: 'a1', country: 'US', label: null, kind: 'direct', path: ['US'], source_label: 'USA', variant: null },
  { id: 'a2', country: 'SE', label: null, kind: 'direct', path: ['SE'], source_label: 'Sweden A', variant: 'A' },
  { id: 'a3', country: 'SE', label: null, kind: 'direct', path: ['SE'], source_label: 'Sweden L', variant: 'L' },
  { id: 'a4', country: 'RU', label: null, kind: 'direct', path: ['RU'], source_label: 'Russia', variant: null },
  { id: 'a5', country: 'US', label: null, kind: 'cascade', path: ['RU', 'US'], source_label: 'USA Cascade', variant: null },
];
const basic = { kind: 'basic', ends_at: now + 21 * day, quota_bytes: 100 * 2 ** 30, used_bytes: 37 * 2 ** 30, bonus_bytes: 25 * 2 ** 30 };
const sub = e => ({ state: 'active', entitlement: e, can_connect: true, effective_id: 'vpn_1', profiles: [{ id: 'vpn_1', entitlement: e, state: 'active', queued: 0 }] });
const signedIn = { signed_in: true, account: { id: 'acc', identities: [{ kind: 'telegram', display: '@te***', id: 'i1' }], vpn_sources: [['vpn_1', 'bot']] }, subscription: sub(basic), config_ready: true, servers, from_account: true };
const base = { signed_in: false, account: null, subscription: null, engine: 'disconnected', wanted: false, engine_error: null, restarts: 0, helper_ok: true, storage_ok: true, config_ready: false,
  active_route: null, traffic: 'unknown', live_ms: null, servers: [], from_account: false, choice: null, route_missing: false, availability: {}, sync_error: null, syncing: false,
  telegram: { state: 'idle' }, email: { state: 'enter_email' }, checkout: { state: 'idle' }, prefs: { theme: q.get('theme') || 'light', language, welcome_seen: true, server: null, routes: null }, version: '0.1.2-5', is_test_build: false };
const S = {
  welcome: { prefs: { ...base.prefs, welcome_seen: false } },
  home_out: {},
  home_off: signedIn,
  home_on: { ...signedIn, engine: 'connected', wanted: true, traffic: 'ok', live_ms: 84, active_route: 'srv-a1', choice: 'a1' },
  home_auto: { ...signedIn, engine: 'connected', wanted: true, traffic: 'ok', live_ms: 84, active_route: 'srv-a3' },
  home_connecting: { ...signedIn, engine: 'connecting', wanted: true },
  home_reconnecting: { ...signedIn, engine: 'connecting', wanted: true, restarts: 3, engine_error: 'engine_exit' },
  home_blocked: { ...signedIn, engine: 'connecting', wanted: true, restarts: 3, engine_error: 'engine_exit', kill_switch: 'armed' },
  home_ks_failed: { ...signedIn, engine: 'disconnected', engine_error: 'killswitch_failed' },
  telegram_blocked: { engine: 'connecting', wanted: true, kill_switch: 'armed' },
  settings_blocked: { ...signedIn, engine: 'connecting', wanted: true, restarts: 3, kill_switch: 'armed', engine_error: 'engine_exit' },
  settings_ks_off: { ...signedIn, prefs: { ...base.prefs, kill_switch: false } },
  home_other_user: { ...signedIn, engine: 'connecting', wanted: true, kill_switch: 'armed', session: 'other' },
  home_lost: { ...signedIn, engine: 'connected', wanted: true, traffic: 'lost', active_route: 'srv-a1', choice: 'a1' },
  home_nonet: { ...signedIn, engine: 'connected', wanted: true, traffic: 'no_network', active_route: 'srv-a1', choice: 'a1' },
  home_nohelper: { ...signedIn, engine: 'unknown', helper_ok: false },
  quit_confirm: { ...signedIn, engine: 'connected', wanted: true, traffic: 'ok', live_ms: 84, active_route: 'srv-a1', choice: 'a1', quit: { state: 'confirm' } },
  quit_progress: { ...signedIn, engine: 'connected', wanted: true, traffic: 'ok', live_ms: 84, active_route: 'srv-a1', choice: 'a1', quit: { state: 'disconnecting' } },
  quit_failed: { ...signedIn, engine: 'connecting', wanted: true, kill_switch: 'armed', restarts: 1, quit: { state: 'failed', code: 'killswitch_remove_failed' } },
  home_mismatch: { ...signedIn, helper_mismatch: true },
  servers_unknown: { ...signedIn, servers: [...servers, { id: 'z1', country: 'ZZ', label: null, kind: 'direct', path: ['ZZ'], source_label: 'Elsewhere', variant: null }, { id: 'z2', country: null, label: 'No country', kind: null, path: [], source_label: 'No country', variant: null }] },
  home_unlimited: { ...signedIn, subscription: sub({ kind: 'unlimited', ends_at: now + 30 * day, quota_bytes: null, used_bytes: null, bonus_bytes: null }) },
  home_trial: { ...signedIn, subscription: sub({ kind: 'trial', ends_at: now + 2 * day, quota_bytes: 3 * 2 ** 30, used_bytes: 2 ** 30, bonus_bytes: null }) },
  home_expired: { ...signedIn, config_ready: false, subscription: { ...sub({ ...basic, ends_at: now - 2 * day }), state: 'expired', can_connect: false } },
  home_nosub: { ...signedIn, config_ready: false, subscription: { state: 'none', entitlement: null, can_connect: false, effective_id: null, profiles: [] } },
  servers: { ...signedIn, engine: 'connected', wanted: true, traffic: 'ok', live_ms: 84, active_route: 'srv-a2', choice: 'a2', availability: { a1: 'ok', a2: 'ok', a3: 'failed' } },
  telegram: {},
  telegram_wait: { telegram: { state: 'waiting', telegram_url: 'https://t.me/MarzReality_bot', match_code: '47', expires_at: now + 262 } },
  email: {},
  email_code: { email: { state: 'enter_code', email: 'user@example.com', sent_at: now - 20, busy: false, error: 'invalid_code' } },
  plans: signedIn,
  plans_out: {},
  settings: signedIn,
  settings_update_checking: { ...signedIn, update: { state: 'checking' } },
  settings_update_latest: { ...signedIn, update: { state: 'latest' } },
  settings_update_available: { ...signedIn, update: { state: 'available', version: '0.1.2-7' } },
  settings_update_failed: { ...signedIn, update: { state: 'failed' } },
  home_update: { ...signedIn, engine: 'connected', wanted: true, traffic: 'ok', live_ms: 84, active_route: 'srv-a1', choice: 'a1', kill_switch: 'armed', update: { state: 'available', version: '0.1.2-7' } },
};
const SCREEN = { servers: 'servers', servers_unknown: 'servers', telegram: 'telegram', telegram_blocked: 'telegram', telegram_wait: 'telegram', email: 'email', email_code: 'email', plans: 'plans', plans_out: 'plans', settings_blocked: 'settings', settings_ks_off: 'settings', settings: 'settings', settings_update_checking: 'settings', settings_update_latest: 'settings', settings_update_available: 'settings', settings_update_failed: 'settings' };
const view = { ...base, ...S[scenario] };
window.__TAURI_INTERNALS__ = { invoke: async cmd => cmd === 'get_state' ? view : undefined };
window.__MARZ_START_SCREEN__ = SCREEN[scenario] || null;
