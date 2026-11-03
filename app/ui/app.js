// MARZ VPN desktop UI: renders the controller's state, screen for screen as Android RC6 (ui/screens/*).
// All behaviour lives in Rust; here only texts, layout and which action a button calls.
const invoke = (cmd, args) => window.__TAURI_INTERNALS__.invoke(cmd, args);
const LANGS = ['ru', 'en', 'fa', 'zh-CN'];
const NAMES = { ru: 'Русский', en: 'English', fa: 'فارسی', 'zh-CN': '简体中文' };
const LOGO = `<svg viewBox="0 0 48 48" width="SIZE" height="SIZE" aria-hidden="true"><path fill="#D5F15F" d="M13,0h22a13,13 0,0 1,13 13v22a13,13 0,0 1,-13 13h-22a13,13 0,0 1,-13 -13v-22a13,13 0,0 1,13 -13z"/><path fill="#20231F" d="M11,36V12h6l7,11 7,-11h6v24h-6V22l-7,10 -7,-10v14z"/></svg>`;
const P = {
  power: 'M13 3h-2v10h2V3zm4.83 2.17l-1.42 1.42C17.99 7.86 19 9.81 19 12c0 3.87-3.13 7-7 7s-7-3.13-7-7c0-2.19 1.01-4.14 2.58-5.42L6.17 5.17C4.23 6.82 3 9.26 3 12c0 4.97 4.03 9 9 9s9-4.03 9-9c0-2.74-1.23-5.18-3.17-6.83z',
  settings: 'M19.14 12.94c.04-.3.06-.61.06-.94 0-.32-.02-.64-.07-.94l2.03-1.58c.18-.14.23-.41.12-.61l-1.92-3.32c-.12-.22-.37-.29-.59-.22l-2.39.96c-.5-.38-1.03-.7-1.62-.94l-.36-2.54c-.04-.24-.24-.41-.48-.41h-3.84c-.24 0-.43.17-.47.41l-.36 2.54c-.59.24-1.13.57-1.62.94l-2.39-.96c-.22-.08-.47 0-.59.22L2.74 8.87c-.12.21-.08.47.12.61l2.03 1.58c-.05.3-.09.63-.09.94s.02.64.07.94l-2.03 1.58c-.18.14-.23.41-.12.61l1.92 3.32c.12.22.37.29.59.22l2.39-.96c.5.38 1.03.7 1.62.94l.36 2.54c.05.24.24.41.48.41h3.84c.24 0 .44-.17.47-.41l.36-2.54c.59-.24 1.13-.56 1.62-.94l2.39.96c.22.08.47 0 .59-.22l1.92-3.32c.12-.22.07-.47-.12-.61l-2.01-1.58zM12 15.6c-1.98 0-3.6-1.62-3.6-3.6s1.62-3.6 3.6-3.6 3.6 1.62 3.6 3.6-1.62 3.6-3.6 3.6z',
  back: 'M20 11H7.83l5.59-5.59L12 4l-8 8 8 8 1.41-1.41L7.83 13H20v-2z',
  chevron: 'M10 6L8.59 7.41 13.17 12l-4.58 4.59L10 18l6-6z',
  send: 'M2.01 21L23 12 2.01 3 2 10l15 2-15 2z',
  email: 'M20 4H4c-1.1 0-1.99.9-1.99 2L2 18c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V6c0-1.1-.9-2-2-2zm0 4l-8 5-8-5V6l8 5 8-5v2z',
  lock: 'M18 8h-1V6c0-2.76-2.24-5-5-5S7 3.24 7 6v2H6c-1.1 0-2 .9-2 2v10c0 1.1.9 2 2 2h12c1.1 0 2-.9 2-2V10c0-1.1-.9-2-2-2zm-6 9c-1.1 0-2-.9-2-2s.9-2 2-2 2 .9 2 2-.9 2-2 2zm3.1-9H8.9V6c0-1.71 1.39-3.1 3.1-3.1 1.71 0 3.1 1.39 3.1 3.1v2z',
  gift: 'M20 6h-2.18c.11-.31.18-.65.18-1 0-1.66-1.34-3-3-3-1.05 0-1.96.54-2.5 1.35l-.5.67-.5-.68C10.96 2.54 10.05 2 9 2 7.34 2 6 3.34 6 5c0 .35.07.69.18 1H4c-1.11 0-1.99.89-1.99 2L2 19c0 1.11.89 2 2 2h16c1.11 0 2-.89 2-2V8c0-1.11-.89-2-2-2zm-5-2c.55 0 1 .45 1 1s-.45 1-1 1-1-.45-1-1 .45-1 1-1zM9 4c.55 0 1 .45 1 1s-.45 1-1 1-1-.45-1-1 .45-1 1-1zm11 15H4v-2h16v2zm0-5H4V8h5.08L7 10.83 8.62 12 11 8.76l1-1.36 1 1.36L15.38 12 17 10.83 14.92 8H20v6z',
  language: 'M11.99 2C6.47 2 2 6.48 2 12s4.47 10 9.99 10C17.52 22 22 17.52 22 12S17.52 2 11.99 2zm6.93 6h-2.95c-.32-1.25-.78-2.45-1.38-3.56 1.84.63 3.37 1.91 4.33 3.56zM12 4.04c.83 1.2 1.48 2.53 1.91 3.96h-3.82c.43-1.43 1.08-2.76 1.91-3.96zM4.26 14C4.1 13.36 4 12.69 4 12s.1-1.36.26-2h3.38c-.08.66-.14 1.32-.14 2 0 .68.06 1.34.14 2H4.26zm.82 2h2.95c.32 1.25.78 2.45 1.38 3.56-1.84-.63-3.37-1.9-4.33-3.56zm2.95-8H5.08c.96-1.66 2.49-2.93 4.33-3.56C8.81 5.55 8.35 6.75 8.03 8zM12 19.96c-.83-1.2-1.48-2.53-1.91-3.96h3.82c-.43 1.43-1.08 2.76-1.91 3.96zM14.34 14H9.66c-.09-.66-.16-1.32-.16-2 0-.68.07-1.35.16-2h4.68c.09.65.16 1.32.16 2 0 .68-.07 1.34-.16 2zm.25 5.56c.6-1.11 1.06-2.31 1.38-3.56h2.95c-.96 1.65-2.49 2.93-4.33 3.56zM16.36 14c.08-.66.14-1.32.14-2 0-.68-.06-1.34-.14-2h3.38c.16.64.26 1.31.26 2s-.1 1.36-.26 2h-3.38z',
  info: 'M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-6h2v6zm0-8h-2V7h2v2z',
  doc: 'M14 2H6c-1.1 0-1.99.9-1.99 2L4 20c0 1.1.89 2 1.99 2H18c1.1 0 2-.9 2-2V8l-6-6zm2 16H8v-2h8v2zm0-4H8v-2h8v2zm-3-5V3.5L18.5 9H13z',
};
const icon = (n, flip) => `<svg class="i${flip ? ' flip' : ''}" viewBox="0 0 24 24" aria-hidden="true"><path d="${P[n]}"/></svg>`;
const esc = s => String(s ?? '').replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
const ltr = v => '⁦' + v + '⁩';

let S = null, T = {}, EN = {}, lang = 'en', screen = null, stack = [], ui = { email: '', code: '', plansAction: 'buy', profile: null, dialog: null, linkEmail: '', linkCode: '', linkSent: false, linkError: null };
const handlers = new Map();

function t(key, ...args) {
  let text = T[key] ?? EN[key] ?? key;
  args.forEach((a, i) => { text = text.replace(new RegExp(`%${i + 1}\\$[ds]`, 'g'), a); });
  return text;
}
const on = fn => { const id = 'h' + handlers.size; handlers.set(id, fn); return `data-h="${id}"`; };
const act = (cmd, args) => invoke(cmd, args).catch(() => {});
const btn = (cls, text, fn, opts = {}) => `<button class="btn ${cls}" ${opts.disabled ? 'disabled' : on(fn)}>${opts.icon ? icon(opts.icon) : ''}<span>${esc(text)}</span></button>`;
const big = (text, fn, opts) => btn('primary', text, fn, opts);
const outlined = (text, fn, opts) => btn('outline', text, fn, opts);
const lime = (text, fn, opts) => btn('lime', text, fn, opts);
const textBtn = (text, fn, opts) => btn('text', text, fn, opts);
const notice = text => `<div class="notice">${icon('info')}<span>${esc(text)}</span></div>`;
const open = url => act('open_external', { url });
const LINKS = { bot: 'https://t.me/MarzReality_bot', mail: 'mailto:support@marzvpn.com?subject=MARZ%20VPN%20App', license: 'https://www.gnu.org/licenses/gpl-3.0.html' };
const sitePrefix = () => ({ ru: '/ru', fa: '/fa', 'zh-CN': '/zh-cn' }[lang] ?? '');
const site = path => `https://marzvpn.com${sitePrefix()}${path}`;

// ---- formats, as Android Components.kt ----------------------------------------------------------
function formatBytes(bytes) {
  const gib = bytes / 1024 ** 3, ru = lang === 'ru';
  const n = (v, d) => new Intl.NumberFormat(lang, { minimumFractionDigits: d, maximumFractionDigits: d }).format(v);
  if (gib >= 1) return `${n(gib, gib >= 10 || gib === Math.floor(gib) ? 0 : 1)} ${ru ? 'ГиБ' : 'GiB'}`;
  return `${n(bytes / 1024 ** 2, 0)} ${ru ? 'МиБ' : 'MiB'}`;
}
const formatDate = sec => new Intl.DateTimeFormat(lang, { dateStyle: 'medium' }).format(new Date(sec * 1000));
const SHORT = { US: { ru: 'США', en: 'USA', fa: 'آمریکا', 'zh-CN': '美国' } };
function countryName(code) {
  const c = (code || '').toUpperCase();
  if (SHORT[c]?.[lang]) return SHORT[c][lang];
  try { return new Intl.DisplayNames([lang], { type: 'region' }).of(c) || c; } catch { return c; }
}
function flag(code) {
  const c = (code || '').toUpperCase();
  if (!/^[A-Z]{2}$/.test(c)) return '🌐';
  return String.fromCodePoint(0x1F1E6 + c.charCodeAt(0) - 65, 0x1F1E6 + c.charCodeAt(1) - 65);
}
const routeTitle = s => s.label ?? (s.country.length === 2 ? (s.variant ? `${countryName(s.country)} ${s.variant}` : countryName(s.country)) : s.id);
function routeDetail(s, all) {
  if (s.kind === 'cascade' && s.path.length >= 2) {
    const sep = lang === 'fa' ? ' ← ' : ' → ';
    return t('route_cascade', s.path.map(c => '⁨' + countryName(c) + '⁩').join(sep));
  }
  if (s.kind === 'direct') return t('route_direct');
  if (s.kind === 'alternative') return t('route_alternative');
  if (!s.country) return null;
  const same = all.filter(x => x.country === s.country), untyped = same.filter(x => !x.kind);
  if (untyped.length > 1) return t('route_variant', untyped.findIndex(x => x.id === s.id) + 1, untyped.length);
  return same.length > 1 ? t('route_other') : null;
}
const routeFull = (s, all) => [routeTitle(s), routeDetail(s, all)].filter(Boolean).join(' · ');
const activeServer = () => { const a = S.active_route?.replace(/^srv-/, ''); return a ? S.servers.find(x => x.id === a) : null; };

function loginError(code, entered) {
  return t({
    upstream_unavailable: entered ? 'error_email_retry_code' : 'error_network', internal_error: entered ? 'error_email_retry_code' : 'error_network',
    network: entered ? 'error_email_retry_code' : 'error_network', email_unavailable: 'error_email_disabled', email_not_configured: 'error_email_disabled',
    code_expired: 'error_email_code_expired', login_expired: 'error_expired', login_rejected: 'error_rejected', login_match_failed: 'error_match_failed',
    pilot_closed: 'error_pilot_closed', email_login_disabled: 'error_email_disabled', storage_unavailable: 'desktop_storage_unavailable', invalid_code: 'error_invalid_code',
    too_many_attempts: 'error_too_many', rate_limited: 'error_too_many', invalid_email: 'error_invalid_email',
  }[code] ?? 'error_generic');
}

// ---- frame -------------------------------------------------------------------------------------
const wordmark = () => `${LOGO.replaceAll('SIZE', 30)}<span>MARZ VPN</span>`;
function frame(title, body, { back = true, actions = '', raw = false } = {}) {
  return `<div class="topbar">${back ? `<button class="iconbtn" aria-label="${esc(t('back'))}" ${on(goBack)}>${icon('back', true)}</button>` : ''}
    <div class="title">${title === null ? wordmark() : esc(title)}</div>${actions}</div>
    ${raw ? body : `<div class="scroll"><div class="column">${body}</div></div>`}`;
}
function go(next) {
  stack.push(screen); screen = next;
  if (next === 'servers') act('route_check_start'); // once per opening, as RC6
  if (next === 'telegram') act('telegram_resume');
  render();
}
function goBack() {
  if (screen === 'servers') act('route_check_stop');
  if (screen === 'telegram') act('telegram_cancel');
  screen = stack.pop() ?? 'home';
  render();
}

// ---- welcome ------------------------------------------------------------------------------------
function trialCard() {
  return `<div class="card"><p class="t-l">${esc(t('trial_title'))}</p><p class="b-l">${esc(t('trial_terms', 3, formatBytes(3 * 1024 ** 3)))}</p><p class="b-m muted">${esc(t('trial_preview_note'))}</p></div>`;
}
const leaveWelcome = next => { act('mark_welcome_seen'); screen = 'home'; stack = []; if (next) go(next); else render(); };
function welcome() {
  return `<div class="scroll"><div class="column center">
    ${S.is_test_build ? `<span class="badge" style="align-self:center">${esc(t('desktop_preview_build'))}</span>` : ''}
    <div style="height:12px"></div>${LOGO.replaceAll('SIZE', 88)}
    <p class="display">MARZ VPN</p><p class="t-m muted">${esc(t('welcome_tagline'))}</p>
    <div style="width:100%;display:flex;flex-direction:column;gap:8px;padding:4px 0">
      ${['welcome_point_one_tap', 'welcome_point_no_setup', 'welcome_point_one_account'].map(k => `<div class="bullet b-l">${esc(t(k))}</div>`).join('')}
    </div>
    ${big(t('login_telegram'), () => leaveWelcome('telegram'), { icon: 'send' })}
    ${outlined(t('login_email'), () => leaveWelcome('email'), { icon: 'email' })}
    ${trialCard()}
    ${lime(t('view_plans'), () => { ui.plansAction = 'buy'; leaveWelcome('plans'); })}
    ${textBtn(t('continue_without_login'), () => leaveWelcome())}
  </div></div>`;
}

// ---- home ---------------------------------------------------------------------------------------
function connectSection(size) {
  const e = S.engine, connected = e === 'connected';
  const noData = connected && (S.traffic === 'lost' || S.traffic === 'no_network');
  const canConnect = S.config_ready || (e !== 'disconnected' && e !== 'unknown');
  const busy = e === 'connecting' || e === 'disconnecting' || (e === 'unknown' && S.helper_ok);
  const reconnecting = e === 'connecting' && S.restarts > 0;
  const [c, onC] = !canConnect || !S.helper_ok ? ['var(--surface-variant)', 'var(--on-surface-variant)']
    : noData ? ['var(--error)', 'var(--on-error)'] : (connected || reconnecting) ? ['var(--connected)', 'var(--on-connected)'] : ['var(--idle)', 'var(--on-idle)'];
  const label = !S.helper_ok ? t('status_disconnected') : reconnecting ? t('desktop_reconnecting') : t({
    unknown: 'status_checking', disconnected: 'status_disconnected', connecting: 'status_connecting', disconnecting: 'status_disconnecting',
    connected: S.traffic === 'lost' ? 'status_no_traffic' : S.traffic === 'no_network' ? 'status_no_network' : 'status_connected',
  }[e]);
  const other = S.session === 'other';
  const clickable = S.helper_ok && !other && canConnect && e !== 'disconnecting' && e !== 'unknown';
  let hint = null, hintErr = false;
  if (!S.helper_ok) { hint = t('desktop_helper_missing'); hintErr = true; }
  else if (other) { hint = t('desktop_other_user'); hintErr = true; }
  else if (!S.storage_ok) { hint = t('desktop_storage_unavailable'); hintErr = true; }
  else if (S.engine_error === 'killswitch_lost') { hint = t('desktop_protection_lost'); hintErr = true; }
  else if (S.engine_error === 'killswitch_failed' || S.engine_error === 'killswitch_no_firewall' || S.engine_error === 'killswitch_state') { hint = t('desktop_protection_failed'); hintErr = true; }
  else if (S.engine_error === 'endpoint_unresolved') { hint = t('desktop_no_address'); hintErr = true; }
  else if (S.engine_error && !reconnecting) { hint = t('desktop_engine_error', S.engine_error); hintErr = true; }
  else if (connected && S.traffic === 'lost') { hint = t('no_traffic_hint'); hintErr = true; }
  else if (!S.config_ready && S.signed_in) hint = t('connect_no_plan');
  else if (!S.config_ready) hint = t('connect_need_login');
  return `<div class="connect"><button class="power${busy ? ' busy' : ''}" style="--c:${c};--on:${onC};--size:${size}px" aria-label="${esc(label)}" ${clickable ? on(() => act('toggle')) : 'disabled'}>
      <span>${icon(canConnect ? 'power' : 'lock')}</span></button>
    <p class="h-s" style="text-align:center">${esc(label)}</p>
    ${hint ? `<p class="b-m ${hintErr || noData ? 'err' : 'muted'}" style="text-align:center">${esc(hint)}</p>` : ''}</div>`;
}
function serverRow() {
  const chosen = S.choice ? S.servers.find(x => x.id === S.choice) : null;
  const active = activeServer(), connected = S.engine === 'connected';
  const missing = S.route_missing || (S.choice && !chosen);
  const ms = S.live_ms != null ? ltr(t('latency_ms', S.live_ms)) : null;
  const main = chosen ? routeFull(chosen, S.servers) : S.choice ? t('route_missing_title') : t('server_auto');
  let sub = '';
  if (missing) sub = `<p class="b-s err">${esc(t('route_missing_text'))}</p>`;
  else if (connected && active && (!S.choice || active.id !== S.choice)) sub = `<p class="b-s muted">${esc(routeFull(active, S.servers) + (ms ? ' · ' + ms : ''))}</p>`;
  else if (connected && ms) sub = `<p class="b-s muted">${esc(ms)}</p>`;
  return `<button class="row${missing ? ' bad' : ''}" ${on(() => go('servers'))}><span class="flag">${chosen ? flag(chosen.country) : S.choice ? '⚠️' : '⚡'}</span>
    <span class="grow"><p class="l-m muted">${esc(t('server_label'))}</p><p class="t-m">${esc(main)}</p>${sub}</span>${icon('chevron', true)}</button>`;
}
const plans = action => { ui.plansAction = action; act('checkout_reset'); go('plans'); };
const expiry = e => `<p class="b-l">${esc(t('subscription_until', formatDate(e.ends_at)))}</p>`;
function traffic(quota, used) {
  const left = Math.max(quota - used, 0);
  return `<div style="display:flex;flex-direction:column;gap:6px"><p class="b-l">${esc(t('traffic_remaining', formatBytes(left), formatBytes(quota)))}</p>
    <div class="progress"><div style="width:${quota > 0 ? left / quota * 100 : 0}%"></div></div></div>`;
}
const renewButtons = () => big(t('renew'), () => plans('renew')) + outlined(t('change_plan'), () => plans('change_plan'));
function subscriptionCard() {
  let body;
  const sub = S.subscription, e = sub?.entitlement;
  if (!S.signed_in) body = `<p class="t-l">${esc(t('subscription_signed_out'))}</p><p class="b-m muted">${esc(t('subscription_signed_out_text'))}</p>
    ${big(t('login'), () => go('login'))}${outlined(t('buy_subscription'), () => plans('buy'))}`;
  else if (!sub) body = `<p class="b-m muted">${esc(t('status_checking'))}</p>`;
  else if (!e) body = `<p class="t-l">${esc(t('subscription_none'))}</p>${lime(t('buy_subscription'), () => plans('buy'))}`;
  else if (sub.state !== 'active') {
    const title = { limited: 'subscription_limited', suspended: 'subscription_suspended' }[sub.state] ?? 'subscription_expired';
    body = `<p class="t-l">${esc(t(title))}</p>`;
    if (sub.state === 'expired') body += `<p class="b-l">${esc(t('subscription_ended', formatDate(e.ends_at)))}</p>`;
    if (sub.state === 'suspended') body += `<p class="b-m muted">${esc(t('subscription_suspended_text'))}</p>`;
    else body += e.kind === 'trial' ? lime(t('buy_subscription'), () => plans('buy')) : lime(t('renew'), () => plans('renew'));
  } else if (e.kind === 'trial') body = `<p class="t-l">${esc(t('trial_title'))}</p>${expiry(e)}${traffic(e.quota_bytes, e.used_bytes)}${lime(t('buy_subscription'), () => plans('buy'))}`;
  else if (e.kind === 'basic') body = `<p class="t-l">Basic</p>${expiry(e)}${traffic(e.quota_bytes, e.used_bytes)}${renewButtons()}`;
  else if (e.kind === 'unlimited') body = `<p class="t-l">Unlimited</p>${expiry(e)}<p class="b-l">${esc(t('plan_no_traffic_limit'))}</p>${renewButtons()}`;
  else body = `<p class="t-l">${esc(t('subscription_active'))}</p>${expiry(e)}${e.quota_bytes != null && e.used_bytes != null ? traffic(e.quota_bytes, e.used_bytes) : ''}${renewButtons()}`;
  return `<div class="card"><p class="l-m muted">${esc(t('subscription_title'))}</p>${body}</div>`;
}
function bonusCard() {
  const bonus = S.subscription?.entitlement?.bonus_bytes;
  return `<div class="card"><div style="display:flex;align-items:center;gap:10px;color:var(--primary)">${icon('gift')}<p class="t-m" style="color:var(--on-bg)">${esc(t('bonus_title'))}</p></div>
    ${S.signed_in && bonus != null ? `<p class="b-l">${esc(t('bonus_balance', formatBytes(bonus)))}</p>` : ''}
    <p class="b-m muted">${esc(t('bonus_text', formatBytes(25 * 1024 ** 3)))}</p>
    ${outlined(t('referral_open_bot'), () => open(LINKS.bot), { icon: 'send' })}<p class="b-s muted">${esc(t('referral_bot_hint'))}</p></div>`;
}
function syncNotice() {
  if (!S.sync_error) return '';
  if (S.sync_error === 'reauth_required') return `<div class="card"><p class="b-l">${esc(t('reauth_text'))}</p>${big(t('login'), () => go('login'))}</div>`;
  return notice(t('sync_error'));
}
function home() {
  const actions = `<button class="iconbtn" aria-label="${esc(t('settings_title'))}" ${on(() => go('settings'))}>${icon('settings')}</button>`;
  const details = `${syncNotice()}${serverRow()}${subscriptionCard()}${bonusCard()}`;
  if (window.innerWidth >= 760) {
    return frame(null, `<div class="two"><div class="scroll"><div class="column center">${connectSection(220)}</div></div><div class="scroll"><div class="column">${details}</div></div></div>`, { back: false, actions, raw: true });
  }
  return frame(null, `${syncNotice()}${connectSection(196)}${serverRow()}${subscriptionCard()}${bonusCard()}`, { back: false, actions });
}

// ---- servers ------------------------------------------------------------------------------------
function option(flagText, title, subtitle, selected, trailing, availability, fn) {
  const dot = availability ? `<span class="dot ${availability === 'ok' ? 'ok' : availability === 'failed' ? 'failed' : ''}" role="img" aria-label="${esc(t({ ok: 'route_ok', failed: 'route_failed' }[availability] ?? 'route_unknown'))}"></span>` : '';
  return `<button class="option" role="radio" aria-checked="${selected}" ${on(fn)}><span class="flag">${flagText}</span><span class="grow"><p class="t-m">${esc(title)}</p>${subtitle ? `<p class="b-s muted">${esc(subtitle)}</p>` : ''}</span>
    ${trailing ? `<span class="l-m muted">${esc(trailing)}</span>` : ''}${dot}<span class="radio${selected ? ' on' : ''}"></span></button>`;
}
function servers() {
  const connected = S.engine === 'connected', active = activeServer();
  const collator = new Intl.Collator(lang);
  const list = [...S.servers].sort((a, b) => collator.compare(routeTitle(a), routeTitle(b)));
  const rows = list.map(s => '<div class="divider"></div>' + option(flag(s.country), routeTitle(s), routeDetail(s, S.servers), S.choice === s.id,
    connected && active?.id === s.id ? (S.live_ms != null ? ltr(t('latency_ms', S.live_ms)) : S.traffic === 'unknown' ? t('latency_measuring') : '—') : null,
    S.from_account ? (S.availability[s.id] ?? 'unknown') : null, () => act('select_server', { id: s.id }))).join('');
  return frame(t('servers_title'), `${textBtn(t('route_check_again'), () => act('route_check_start'), { disabled: !(connected && S.from_account) })}
    <div class="card">${option('⚡', t('server_auto'), connected && !S.choice && active ? routeFull(active, S.servers) : t('server_auto_hint'), !S.choice, null, null, () => act('select_server', { id: null }))}${rows}</div>
    ${!S.from_account ? notice(t('connect_need_login')) : !connected ? `<p class="b-s muted">${esc(t('route_check_needs_vpn'))}</p>` : ''}`);
}

// ---- sign-in ------------------------------------------------------------------------------------
function login() {
  return frame(t('login'), `<p class="b-l">${esc(t('login_chooser_text'))}</p>
    ${big(t('login_telegram'), () => go('telegram'), { icon: 'send' })}<p class="b-m muted">${esc(t('login_chooser_telegram_hint'))}</p>
    ${outlined(t('login_email'), () => go('email'), { icon: 'email' })}<p class="b-m muted">${esc(t('login_chooser_email_hint'))}</p>`);
}
function telegram() {
  const st = S.telegram;
  if (st.state === 'waiting') {
    const left = Math.max(st.expires_at - Math.floor(Date.now() / 1000), 0);
    const clock = `${Math.floor(left / 60)}:${String(left % 60).padStart(2, '0')}`;
    return frame(t('login_telegram'), `<p class="t-m">${esc(t('login_telegram_remember'))}</p>
      <div class="card hl"><p class="t-m" style="text-align:center">${esc(t('login_telegram_code_title'))}</p><div class="code">${esc(st.match_code)}</div>
      <p class="b-m" style="text-align:center">${esc(t('login_telegram_expires_in', ltr(clock)))}</p></div>
      <p class="b-m">${esc(t('login_telegram_code_hint'))}</p>
      ${big(t('login_telegram_open'), () => open(st.telegram_url), { icon: 'send' })}
      <div class="waiting"><span class="spinner"></span>${esc(t('login_telegram_waiting'))}</div>
      ${textBtn(t('cancel'), () => act('telegram_cancel'))}`);
  }
  const failed = st.state === 'failed';
  return frame(t('login_telegram'), `<p class="b-l">${esc(t('login_telegram_intro'))}</p>
    <div class="card">${[1, 2, 3].map(n => `<div class="step"><b>${new Intl.NumberFormat(lang).format(n)}</b><span class="b-l">${esc(t('login_telegram_step' + n))}</span></div>`).join('')}</div>
    ${failed ? `<p class="b-m err">${esc(loginError(st.code))}</p>` : ''}
    ${big(t(failed ? 'login_telegram_new_code' : 'login_telegram_get_code'), () => act('telegram_start'), { disabled: st.state === 'starting' })}
    ${st.state === 'starting' ? '<div class="spinner"></div>' : ''}`);
}
function email() {
  const st = S.email;
  if (st.state === 'enter_code') {
    const left = Math.max(st.sent_at + 60 - Math.floor(Date.now() / 1000), 0);
    return frame(t('login_email'), `<p class="b-l">${esc(t('login_email_code_sent', ltr(st.email)))}</p>
      <input class="field" id="code" type="password" inputmode="numeric" autocomplete="one-time-code" maxlength="6" placeholder="${esc(t('login_email_code_label'))}" value="${esc(ui.code)}">
      ${st.error ? `<p class="b-m err">${esc(loginError(st.error, true))}</p>` : ''}
      ${btn('primary', t('login_email_verify'), () => { act('email_verify', { code: ui.code }); ui.code = ''; }, { disabled: st.busy })}
      ${left > 0 ? `<p class="b-m muted">${esc(t('login_email_resend_wait', left))}</p>` : textBtn(t('login_email_resend'), () => { ui.code = ''; act('email_resend', { locale: apiLocale() }); }, { disabled: st.busy })}
      ${textBtn(t('login_email_change'), () => { ui.code = ''; act('email_reset'); })}
      <p class="b-m muted">${esc(t('login_email_hint'))}</p>`);
  }
  return frame(t('login_email'), `<p class="b-l">${esc(t('login_email_intro'))}</p>
    <input class="field" id="email" type="email" autocomplete="email" placeholder="${esc(t('email_label'))}" value="${esc(ui.email)}">
    ${st.state === 'failed' ? `<p class="b-m err">${esc(loginError(st.code))}</p>` : ''}
    ${btn('primary', t('login_email_send_code'), () => { if (ui.email.includes('@') && ui.email.length >= 5) act('email_send', { email: ui.email, locale: apiLocale() }); }, { icon: 'email', disabled: st.state === 'sending' })}
    <p class="b-m muted">${esc(t('login_email_hint'))}</p><p class="b-m muted">${esc(t('login_email_no_trial'))}</p>`);
}
const apiLocale = () => ({ ru: 'ru', fa: 'fa', 'zh-CN': 'zh-cn' }[lang] ?? 'en');

// ---- plans --------------------------------------------------------------------------------------
const CATALOG = [{ code: 'basic', price: '2', days: 30, quota: 100 * 1024 ** 3 }, { code: 'unlimited', price: '4', days: 30, quota: null }];
function planCard(plan, label, fn, { enabled = true, current = false } = {}) {
  const unl = plan.code === 'unlimited', name = unl ? 'Unlimited' : 'Basic';
  const bullets = [plan.quota == null ? t('plan_no_traffic_limit') : t('plan_traffic', formatBytes(plan.quota)), t('plan_all_servers'), ...(unl ? [t('plan_no_speed_cap'), t('plan_ip3')] : [t('plan_basic_bonus')])];
  return `<div class="card${unl ? ' hl' : ''}">${current || unl ? `<span class="badge">${esc(t(current ? 'pay_your_plan' : 'plan_unlimited_badge'))}</span>` : ''}
    <p class="h-s">${name}</p><div style="display:flex;align-items:flex-end;gap:6px"><span class="price">$${plan.price}</span><span class="t-m muted" style="padding-bottom:6px">${esc(t('plan_period', plan.days))}</span></div>
    ${bullets.map(b => `<div class="bullet b-l">${esc(b)}</div>`).join('')}
    ${(unl ? lime : big)(label, fn, { disabled: !enabled })}</div>`;
}
function checkoutStatus() {
  const c = S.checkout;
  if (c.state === 'requesting') return `<div class="waiting"><span class="spinner"></span>${esc(t('pay_preparing'))}</div>`;
  if (c.state === 'opened') {
    const line = { pending: 'pay_waiting', fulfilling: 'pay_receipt_fulfilling', failed: 'pay_receipt_failed', expired: 'pay_receipt_failed', cancelled: 'pay_receipt_failed' }[c.order];
    return `<div class="card"><p class="b-l">${esc(t(c.channel === 'telegram_bot' ? 'pay_opened_bot' : 'pay_opened_site'))}</p>
      <p class="b-m muted">${esc(t(line ?? 'pay_no_order'))}</p>
      ${outlined(t('pay_check_now'), () => act('checkout_check'), { disabled: c.checking })}
      ${['awaiting_checkout', 'pending', 'unavailable'].includes(c.order) ? big(t('pay_return'), () => act('checkout_retry')) : ''}
      ${['failed', 'expired', 'cancelled'].includes(c.order) ? big(t('pay_new_link'), () => act('checkout_retry')) : ''}
      ${c.order !== 'fulfilling' ? outlined(t('pay_choose_other'), () => act('checkout_reset')) : ''}</div>`;
  }
  if (c.state === 'confirmed') {
    const e = S.subscription?.entitlement;
    return `<div class="card hl"><p class="t-m">${esc(t('pay_confirmed'))}</p>${e ? expiry(e) : ''}${big(t('pay_done'), () => { act('checkout_reset'); goBack(); })}</div>`;
  }
  if (c.state === 'failed') {
    const off = { bot: 'pay_err_off_bot', site: 'pay_err_off_site' }[c.source] ?? 'pay_err_off_unknown';
    const text = { feature_off: 'pay_err_off', signed_out: 'pay_err_signed_out', pilot_closed: 'error_pilot_closed', profile_gone: 'pay_err_profile_gone', choose_profile: 'pay_err_choose',
      no_profile: c.has_profiles ? 'pay_err_no_profile' : 'pay_err_no_profile_first', review: 'pay_err_review', unavailable: 'pay_err_unavailable', network: 'pay_err_network', too_many: 'error_too_many',
      clock: 'pay_err_clock', expired: 'pay_err_expired' }[c.problem] ?? 'pay_err_generic';
    return `<div class="card"><p class="b-l err">${esc(t(text))}</p>${c.problem === 'feature_off' ? `<p class="b-m muted">${esc(t(off))}</p>` : ''}
      ${c.problem === 'review' ? outlined(t('pay_contact_support'), () => open(LINKS.mail)) : ''}
      ${c.problem === 'feature_off' && c.source !== 'site' ? outlined(t('referral_open_bot'), () => open(LINKS.bot), { icon: 'send' }) : ''}
      ${c.problem === 'feature_off' && c.source !== 'bot' ? outlined(t('pay_open_site'), () => open(site('/'))) : ''}
      ${['expired', 'network', 'unavailable', 'generic'].includes(c.problem) ? big(t('pay_new_link'), () => act('checkout_retry')) : ''}
      ${outlined(t('pay_choose_other'), () => act('checkout_reset'))}</div>`;
  }
  return '';
}
function plansScreen() {
  if (!S.signed_in) {
    const rows = CATALOG.map(p => planCard(p, t('plan_choose', p.code === 'unlimited' ? 'Unlimited' : 'Basic'), () => { ui.dialog = p.code; render(); })).join('');
    const dialog = ui.dialog ? `<div class="dialog-back"><div class="dialog"><p class="t-l">${esc(t('checkout_dialog_title'))}</p><p class="b-m">${esc(t('checkout_dialog_text'))}</p>
      ${big(t('checkout_dialog_continue'), () => { open(LINKS.bot); ui.dialog = null; render(); })}
      ${outlined(t('pay_open_site'), () => { open(site(`/checkout?plan=${ui.dialog}`)); ui.dialog = null; render(); })}
      ${textBtn(t('cancel'), () => { ui.dialog = null; render(); })}</div></div>` : '';
    return frame(t('plans_title'), `<p class="b-l muted">${esc(t('plans_subtitle'))}</p>${rows}${outlined(t('pay_telegram_stars'), () => open(LINKS.bot), { icon: 'send' })}
      ${notice(t('checkout_preview_note'))}<p class="b-s muted">${esc(t('payment_methods_note'))}</p>`) + dialog;
  }
  const sub = S.subscription, profiles = sub?.profiles ?? [], many = profiles.length > 1;
  const profile = ui.profile ?? (many ? null : sub?.effective_id ?? profiles[0]?.id ?? null);
  const ent = many ? profiles.find(p => p.id === profile)?.entitlement : sub?.entitlement;
  const currentPlan = ent?.kind === 'basic' || ent?.kind === 'unlimited' ? ent.kind : null;
  const titleKey = { renew: 'pay_title_renew', change_plan: 'pay_title_change' }[ui.plansAction] ?? 'pay_title_buy';
  const chooser = many ? `<div class="card"><p class="t-m">${esc(t('pay_choose_profile'))}</p>${profiles.map(p => {
    const e = p.entitlement, name = !e ? t('pay_profile_none') : e.kind === 'trial' ? t('trial_title') : e.kind === 'basic' ? 'Basic' : e.kind === 'unlimited' ? 'Unlimited' : t('subscription_active');
    const state = !e ? '' : p.state === 'active' ? t('subscription_until', formatDate(e.ends_at)) : p.state === 'limited' ? t('subscription_limited') : p.state === 'suspended' ? t('subscription_suspended') : t('subscription_ended', formatDate(e.ends_at));
    const src = (S.account?.vpn_sources ?? []).find(([i]) => i === p.id)?.[1];
    const sub2 = [state, src === 'bot' ? t('pay_profile_bot') : src === 'site' ? t('pay_profile_site') : ''].filter(Boolean).join(' · ');
    return option('', name, sub2, profile === p.id, null, null, () => { ui.profile = p.id; render(); });
  }).join('')}</div>` : '';
  const busy = S.checkout.state === 'requesting' || (S.checkout.state === 'opened' && S.checkout.order === 'fulfilling');
  const rows = CATALOG.map(p => {
    const name = p.code === 'unlimited' ? 'Unlimited' : 'Basic';
    const planAction = ui.plansAction === 'buy' ? 'buy' : currentPlan === p.code ? 'renew' : ui.plansAction === 'renew' && currentPlan ? 'change_plan' : ui.plansAction;
    const label = { renew: t('pay_renew_plan', name), change_plan: t('pay_switch_plan', name), buy: t('plan_choose', name) }[planAction] + ' · ' + t('pay_open_site');
    return planCard(p, label, () => act('checkout_start', { plan: p.code, action: planAction, profile, language: lang, channel: null }), { enabled: !busy && (!many || profile), current: currentPlan === p.code });
  }).join('');
  return frame(t(titleKey), `<p class="b-l muted">${esc(t('pay_hint'))}</p>${checkoutStatus()}${chooser}${S.checkout.state === 'opened' || S.checkout.state === 'confirmed' ? '' : rows}
    <p class="b-s muted">${esc(t('payment_methods_note'))}</p>`);
}

// ---- settings -----------------------------------------------------------------------------------
const setRow = (ic, title, value, fn) => `<button class="setrow" ${on(fn)}><span style="color:var(--primary)">${icon(ic)}</span><span class="grow"><p class="t-m">${esc(title)}</p>${value ? `<p class="b-m muted">${esc(value)}</p>` : ''}</span></button>`;
const langName = l => l === 'system' ? t('language_system') : NAMES[l];
function emailLink() {
  const ids = S.account?.identities ?? [];
  if (!ids.some(i => i.kind === 'telegram')) return '';
  const linked = ids.filter(i => i.kind === 'email');
  if (linked.length) {
    return `<p class="b-m muted">${esc(t('email_link_status'))}</p>` + linked.map(i => outlined(t('email_link_remove', ltr(i.display)), async () => {
      const r = await invoke('email_unlink', { id: i.id }); ui.linkError = r.ok ? null : r.code; render();
    })).join('') + (ui.linkError ? `<p class="b-m err">${esc(loginError(ui.linkError))}</p>` : '');
  }
  if (ui.linkSent) {
    return `<p class="b-m">${esc(t('login_email_code_sent', ltr(ui.linkEmail)))}</p><input class="field" id="linkCode" type="password" inputmode="numeric" maxlength="6" placeholder="${esc(t('login_email_code_label'))}" value="${esc(ui.linkCode)}">
      ${ui.linkError ? `<p class="b-m err">${esc(loginError(ui.linkError, true))}</p>` : ''}
      ${big(t('email_link_confirm'), async () => { const r = await invoke('email_link_confirm', { email: ui.linkEmail, code: ui.linkCode }); ui.linkCode = ''; if (r.ok) { ui.linkSent = false; ui.linkError = null; } else ui.linkError = r.code; render(); })}
      ${textBtn(t('email_link_cancel'), () => { ui.linkSent = false; ui.linkError = null; ui.linkCode = ''; render(); })}`;
  }
  return `<p class="b-m muted">${esc(t('email_link_hint'))}</p><input class="field" id="linkEmail" type="email" placeholder="${esc(t('email_label'))}" value="${esc(ui.linkEmail)}">
    ${ui.linkError ? `<p class="b-m err">${esc(loginError(ui.linkError))}</p>` : ''}
    ${outlined(t('email_link_add'), async () => { const r = await invoke('email_link_start', { email: ui.linkEmail, locale: apiLocale() }); if (r.ok) { ui.linkSent = true; ui.linkError = null; } else ui.linkError = r.code; render(); })}`;
}
// Changes only with the VPN off, so a running tunnel never changes mode. The big button stays the only VPN control.
function killSwitchRow() {
  const on_ = S.prefs.kill_switch !== false, free = S.helper_ok && S.engine === 'disconnected' && !S.wanted;
  return `<button class="ks-row" role="switch" aria-checked="${on_}" ${free ? on(() => act('set_kill_switch', { on: !on_ })) : 'disabled'}>
      <span class="t-m">${esc(t('desktop_ks_title'))}</span><span class="switch${on_ ? ' on' : ''}"><span></span></span></button>
    <p class="b-s ${on_ ? 'muted' : 'err'}">${esc(t(on_ ? 'desktop_ks_on_text' : 'desktop_ks_off_warning'))}</p>
    ${free ? '' : `<p class="b-s muted">${esc(t('desktop_ks_locked'))}</p>`}`;
}
function emergencyDialog() {
  if (ui.dialog !== 'emergency') return '';
  return `<div class="dialog-back"><div class="dialog"><p class="t-l">${esc(t('desktop_emergency_title'))}</p><p class="b-m">${esc(t('desktop_emergency_text'))}</p>
    ${ui.emergencyFailed ? `<p class="b-m err" dir="ltr">${esc(t(S.platform === 'windows' ? 'desktop_emergency_failed_windows' : 'desktop_emergency_failed'))}</p>` : ''}
    ${outlined(t('desktop_emergency_confirm'), async () => {
      const r = await invoke('emergency_off').catch(() => ({ ok: false }));
      ui.emergencyFailed = !r.ok;
      if (r.ok) ui.dialog = null;
      render();
    })}
    ${textBtn(t('cancel'), () => { ui.dialog = null; render(); })}</div></div>`;
}
function settings() {
  const account = S.signed_in
    ? `${(S.account?.identities ?? []).map(i => `<p class="b-l">${esc(t(i.kind === 'telegram' ? 'identity_telegram' : 'identity_email', ltr(i.display)))}</p>`).join('')}${emailLink()}${outlined(t('logout'), () => act('logout'))}`
    : `<p class="t-m">${esc(t('subscription_signed_out'))}</p>${big(t('login'), () => go('login'))}`;
  const dialog = ui.dialog === 'language' ? `<div class="dialog-back"><div class="dialog"><p class="t-l">${esc(t('settings_language'))}</p>
    ${['system', ...LANGS].map(l => `<button class="option" ${on(async () => { ui.dialog = null; await act('set_language', { language: l }); })}><span class="radio${S.prefs.language === l ? ' on' : ''}"></span><span>${esc(langName(l))}</span></button>`).join('')}
    ${textBtn(t('cancel'), () => { ui.dialog = null; render(); })}</div></div>` : '';
  return frame(t('settings_title'), `<p class="section">${esc(t('settings_account'))}</p><div class="card">${account}</div>
    <p class="section">${esc(t('settings_appearance'))}</p><div class="card">${setRow('language', t('settings_language'), langName(S.prefs.language), () => { ui.dialog = 'language'; render(); })}
      <div class="divider"></div><p class="t-m">${esc(t('settings_theme'))}</p><div class="segments" role="radiogroup">${['system', 'light', 'dark'].map(m => `<button role="radio" aria-checked="${S.prefs.theme === m}" class="${S.prefs.theme === m ? 'on' : ''}" ${on(() => act('set_theme', { theme: m }))}>${esc(t('theme_' + m))}</button>`).join('')}</div></div>
    <p class="section">${esc(t('desktop_ks_title'))}</p><div class="card">${killSwitchRow()}</div>
    <p class="section">${esc(t('settings_support'))}</p><div class="card">
      ${setRow('send', t('support_telegram'), '@MarzReality_bot', () => open(LINKS.bot))}<div class="divider"></div>
      ${setRow('email', t('support_email'), 'support@marzvpn.com', () => open(LINKS.mail))}<div class="divider"></div>
      ${setRow('language', t('support_site'), 'marzvpn.com', () => open(site('/')))}<div class="divider"></div>
      ${setRow('doc', t('support_terms'), null, () => open(site('/terms')))}</div>
    <p class="section">${esc(t('settings_about'))}</p><div class="card"><p class="b-l">${esc(t('version', ltr(S.version)))}</p><div class="divider"></div>
      ${setRow('doc', t('license_title'), t('license_value'), () => open(LINKS.license))}<p class="b-s muted">${esc(t('open_source_note'))}</p><p class="b-s muted">${esc(t('desktop_vpn_stays_on'))}</p></div>
    ${S.wanted || !S.helper_ok ? `<div class="card">${textBtn(t('desktop_emergency_title'), () => { ui.dialog = 'emergency'; ui.emergencyFailed = false; render(); })}</div>` : ''}`) + dialog + emergencyDialog();
}

// ---- render -------------------------------------------------------------------------------------
const SCREENS = { welcome, home, servers, login, telegram, email, plans: plansScreen, settings };
// A warning about a connection problem, and nothing else: the big button is the only ordinary control.
function blockedBanner() {
  if (S.kill_switch !== 'armed' || S.engine === 'connected' && S.engine_error !== 'killswitch_lost') return '';
  const lost = S.engine_error === 'killswitch_lost';
  if (!lost && S.session !== 'other' && !(S.restarts > 0 || S.engine_error || !S.config_ready)) return '';
  return `<div class="blocked" role="status"><p class="b-m">${esc(t(lost ? 'desktop_protection_lost' : 'desktop_blocked'))}</p></div>`;
}
function applyPrefs() {
  const wanted = S.prefs.language === 'system' ? systemLang() : S.prefs.language;
  if (wanted !== lang || !T.__ok) return loadLang(wanted).then(render);
  const theme = S.prefs.theme === 'system' ? (matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light') : S.prefs.theme;
  document.documentElement.dataset.theme = theme;
  return null;
}
function systemLang() {
  const l = (navigator.language || 'en').toLowerCase();
  if (l.startsWith('ru')) return 'ru';
  if (l.startsWith('fa')) return 'fa';
  if (l.startsWith('zh')) return 'zh-CN';
  return 'en';
}
async function loadLang(l) {
  const get = async code => (await fetch(`i18n/${code}.json`)).json();
  if (!EN.back) EN = await get('en');
  T = l === 'en' ? { ...EN } : await get(l);
  T.__ok = true;
  lang = l;
  document.documentElement.lang = l;
  document.documentElement.dir = l === 'fa' ? 'rtl' : 'ltr';
}
function render() {
  if (!S || !T.__ok) return;
  if (applyPrefs()) return;
  const active = document.activeElement?.id, pos = active ? document.activeElement.selectionStart : null;
  const scrolls = [...document.querySelectorAll('.scroll')].map(e => e.scrollTop);
  handlers.clear();
  document.getElementById('app').innerHTML = blockedBanner() + SCREENS[screen]();
  [...document.querySelectorAll('.scroll')].forEach((e, i) => { e.scrollTop = scrolls[i] ?? 0; });
  if (active) { const el = document.getElementById(active); if (el) { el.focus(); if (pos != null) try { el.setSelectionRange(pos, pos); } catch {} } }
}
document.addEventListener('click', e => {
  const b = e.target.closest('[data-h]');
  if (b) handlers.get(b.dataset.h)?.();
});
document.addEventListener('input', e => {
  const id = e.target.id, v = e.target.value;
  if (id === 'email') ui.email = v.trim();
  if (id === 'code') { ui.code = v.replace(/\D/g, '').slice(0, 6); e.target.value = ui.code; }
  if (id === 'linkEmail') ui.linkEmail = v.trim();
  if (id === 'linkCode') { ui.linkCode = v.replace(/\D/g, '').slice(0, 6); e.target.value = ui.linkCode; }
});
function update(view) {
  const wasSignedIn = S?.signed_in;
  S = view;
  if (screen === null) screen = window.__MARZ_START_SCREEN__ || (S.prefs.welcome_seen || S.signed_in ? 'home' : 'welcome');
  // A finished sign-in leaves the sign-in screens, as RC6 returns to home.
  if (!wasSignedIn && S.signed_in && ['login', 'telegram', 'email', 'welcome'].includes(screen)) { screen = 'home'; stack = []; }
  if (document.activeElement?.tagName === 'INPUT' && screen !== 'email' && screen !== 'settings') return;
  render();
}
window.addEventListener('resize', () => render());
window.addEventListener('focus', () => act('on_visible', { visible: true }));
window.addEventListener('blur', () => act('on_visible', { visible: false }));
matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => render());
setInterval(() => { if (screen === 'telegram' || screen === 'email') render(); }, 1000);
(async () => {
  await loadLang(systemLang());
  update(await invoke('get_state'));
  window.__MARZ_UPDATE__ = update;
  const poll = async () => { try { update(await invoke('get_state')); } catch {} };
  setInterval(poll, 1000);
})();
