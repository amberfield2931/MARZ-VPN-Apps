// Regression test for the text fields (tests/ui/run-input-tests.py). Loaded after mock.js and app.js in a synthetic
// page, never part of the app package. Screens re-render every second, so each case spans at least one re-render.
const done = async o => { await fetch('/__result', { method: 'POST', body: JSON.stringify(o) }); document.title = 'DONE'; };
(async () => {
  const q = new URLSearchParams(location.search), results = [], wait = ms => new Promise(r => setTimeout(r, ms));
  const check = (name, got, want) => results.push({ name, ok: got === want, got: String(got).slice(0, 80), want: String(want).slice(0, 80) });
  const field = id => document.getElementById(id);
  const insert = text => document.execCommand('insertText', false, text);
  const left = n => { for (let i = 0; i < n; i++) getSelection().modify('move', 'backward', 'character'); };
  const typeChars = async (text, ms) => { for (const c of text) { insert(c); await wait(ms); } };
  const start = async id => { const e = field(id); if (!e) throw new Error('no field ' + id + ' on ' + screen); e.focus(); e.select(); document.execCommand('delete'); await wait(20); return field(id); };
  const open = async (name, st, id) => { Object.assign(view, st); update({ ...view }); screen = name; stack = []; render(); await wait(50); return start(id); };
  const ticks = () => wait(1150);
  while (!S || !T.__ok) await wait(20);
  if (window.__HOLD__) await new Promise(r => { window.__GO__ = r; });
  window.__TAURI_INTERNALS__.invoke = async (cmd, args) => cmd === 'get_state' ? view : undefined;

  const ADDR = 'john.doe+tag_1-x@example-mail.co.uk';
  async function textField(id, model, caseName) {
    let node = await start(id);
    await typeChars(ADDR, 130);
    check(`${caseName} slow typing`, field(id).value, ADDR);
    check(`${caseName} model follows`, ui[model], ADDR);
    check(`${caseName} field never replaced`, field(id) === node, true);
    check(`${caseName} keeps focus`, document.activeElement?.id, id);

    node = await start(id); await typeChars(ADDR, 4); await ticks();
    check(`${caseName} fast typing`, field(id).value, ADDR);

    node = await start(id); insert(ADDR); await ticks();
    check(`${caseName} paste`, field(id).value, ADDR);
    insert('z'); await wait(30);
    check(`${caseName} typing after paste and re-render goes to the end`, field(id).value, ADDR + 'z');

    await start(id); insert('ab@x.co'); await ticks();
    document.execCommand('delete'); await ticks();
    check(`${caseName} Backspace`, field(id).value, 'ab@x.c');
    left(2); await ticks();
    document.execCommand('forwardDelete'); await ticks();
    check(`${caseName} Delete in the middle`, field(id).value, 'ab@xc');

    await start(id); insert('ab@x.co'); left(5); insert('c'); await ticks(); insert('d'); await wait(30);
    check(`${caseName} edit in the middle keeps the caret`, field(id).value, 'abcd@x.co');

    const long = 'a'.repeat(120) + '.b+c_d-e@' + 'sub.'.repeat(20) + 'example.com';
    await start(id); insert(long); await ticks();
    check(`${caseName} long address`, field(id).value, long);

    await start(id); insert('first@'); field(id).blur(); await ticks();
    check(`${caseName} value survives losing focus`, field(id).value, 'first@');
    field(id).focus(); getSelection().modify('move', 'forward', 'documentboundary'); insert('x.co'); await wait(30);
    check(`${caseName} typing after focus returns`, field(id).value.replace(/^x\.co/, ''), 'first@x.co'.replace(/^x\.co/, ''));

    await start(id); insert('reset@x.co'); ui[model] = ''; render(); await wait(30);
    check(`${caseName} app can still clear the field`, field(id).value, '');
    await start(id);
  }

  await wait(100);
  await open('email', { email: { state: 'enter_email' } }, 'email');
  check('email field runs left to right in every language', field('email').getAttribute('dir'), 'ltr');
  await textField('email', 'email', 'email');
  // A state change while typing shows the error without touching the field.
  let node = await start('email'); insert('state@x.co');
  view.email = { state: 'failed', code: 'invalid_email' }; update({ ...view }); await wait(30);
  check('email error appears while typing', !!document.querySelector('.err'), true);
  check('email field survives the state change', field('email') === node && field('email').value === 'state@x.co', true);
  ui.email = 'send@x.co'; let sent = null; window.__TAURI_INTERNALS__.invoke = async (cmd, a) => { if (cmd === 'email_send') sent = a.email; return cmd === 'get_state' ? view : undefined; };
  document.querySelector('button.btn.primary[data-h]')?.click(); await wait(30);
  check('send button still works after a re-render', sent, 'send@x.co');
  window.__TAURI_INTERNALS__.invoke = async cmd => cmd === 'get_state' ? view : undefined;
  // Leaving the screen with the focus in the field: the code screen replaces it.
  view.email = { state: 'enter_code', email: 'state@x.co', sent_at: Math.floor(Date.now() / 1000), busy: false };
  update({ ...view }); await wait(30);
  check('moves on to the code screen', !!field('code') && !field('email'), true);

  node = await start('code');
  await typeChars('12a3-4', 130);
  check('code keeps digits only', field('code').value, '1234');
  check('code field never replaced', field('code') === node, true);
  document.execCommand('delete'); await ticks();
  check('code Backspace', field('code').value, '123');
  field('code').setSelectionRange(1, 1); await ticks(); insert('9'); await wait(30);
  check('code edit in the middle', field('code').value, '1923');
  await typeChars('5678', 10);
  check('code stops at six digits', field('code').value.length, 6);
  check('code digits are visible, not masked', field('code').type, 'text');
  check('code keeps the numeric keyboard', field('code').inputMode, 'numeric');
  await start('code'); insert(' 123-456 '); await wait(30);
  check('code pasted with separators', field('code').value, '123456');
  await ticks();
  check('code survives re-renders', field('code').value, '123456');
  const countdown = [...document.querySelectorAll('.muted')].map(e => e.textContent).join(); await ticks();
  check('resend countdown keeps running while the field is focused', [...document.querySelectorAll('.muted')].map(e => e.textContent).join() !== countdown, true);

  // Telegram code sign-in: the in-app code field (fallback to the page) and the buttons of the code screen.
  const calls = [];
  window.__TAURI_INTERNALS__.invoke = async (cmd, a) => { if (cmd !== 'get_state') calls.push([cmd, a]); return cmd === 'get_state' ? view : undefined; };
  const tgState = (extra = {}) => ({ state: 'code', ref: 'K7QP-9M2X', telegram_url: 'https://t.me/MarzReality_bot?start=login_K7QP9M2X', expires_at: Math.floor(Date.now() / 1000) + 500,
    progress: 'code_sent', attempts_left: 5, can_open_page: true, offline: false, busy: false, error: null, ...extra });
  view.signed_in = false; ui.tgManual = false; screen = 'telegram'; stack = ['home', 'login']; view.telegram = tgState(); update({ ...view }); await wait(50);
  check('tg code: entry hidden until asked', !!field('tgcode'), false);
  check('tg code: request number shown', document.querySelector('.code')?.textContent, 'K7QP-9M2X');
  const press = text => { const b = [...document.querySelectorAll('button[data-h]')].find(x => x.textContent.trim() === text); b?.click(); return !!b; };
  calls.length = 0; press(t('desktop_tg_open_page')); await wait(30);
  check('tg code: open the page again', calls.map(c => c[0]).join(), 'telegram_open_page');
  press(t('desktop_tg_enter_here')); await wait(30);
  node = await start('tgcode');
  await typeChars('12a3 4-5', 130);
  check('tg code keeps digits only', field('tgcode').value, '12345');
  check('tg code field never replaced', field('tgcode') === node, true);
  view.telegram = tgState({ progress: 'delivery_failed' }); update({ ...view }); await wait(30);
  check('tg code field survives a progress change', field('tgcode') === node && field('tgcode').value === '12345', true);
  await typeChars('67', 10);
  check('tg code stops at six digits', field('tgcode').value, '123456');
  check('tg code digits are visible, not masked', field('tgcode').type, 'text');
  check('tg code runs left to right', field('tgcode').getAttribute('dir'), 'ltr');
  await ticks();
  calls.length = 0; press(t('login_email_verify')); await wait(30);
  check('tg code: sign-in button sends the digits', JSON.stringify(calls.find(c => c[0] === 'telegram_code')?.[1]), JSON.stringify({ code: '123456' }));
  view.telegram = tgState({ error: 'invalid_code', attempts_left: 2 }); update({ ...view }); await wait(30);
  check('tg code: wrong code says attempts left', document.querySelector('.err')?.textContent, t('desktop_tg_err_wrong', new Intl.NumberFormat(lang).format(2)));
  view.telegram = tgState({ error: 'code_expired' }); update({ ...view }); await wait(30);
  check('tg code: expired code asks the bot for a new one', document.querySelector('.err')?.textContent, t('desktop_tg_err_code_expired'));
  calls.length = 0; press(t('desktop_tg_use_email')); await wait(30);
  check('tg code: continue with email cancels and opens email', calls.map(c => c[0]).join() + ' ' + screen, 'telegram_cancel email');
  goBack(); await wait(30);
  check('tg code: back from email goes to the chooser', screen, 'login');
  view.telegram = { state: 'idle' }; update({ ...view }); screen = 'telegram'; render(); await wait(30);
  calls.length = 0; press(t('desktop_tg_start')); await wait(30);
  check('tg start sends the page language', JSON.stringify(calls.find(c => c[0] === 'telegram_start')?.[1]), JSON.stringify({ locale: { ru: 'ru', fa: 'fa', 'zh-CN': 'zh-cn' }[lang] ?? 'en' }));
  view.telegram = { state: 'failed', code: 'login_code_attempts' }; update({ ...view }); await wait(30);
  check('tg: too many wrong codes', document.querySelector('.err')?.textContent, t('desktop_tg_err_attempts'));
  view.telegram = { state: 'failed', code: 'upstream_unavailable' }; update({ ...view }); await wait(30);
  check('tg: unavailable suggests email', document.querySelector('.err')?.textContent + '|' + [...document.querySelectorAll('button')].some(b => b.textContent.trim() === t('desktop_tg_use_email')), t('desktop_tg_err_unavailable') + '|true');
  view.telegram = { state: 'idle' }; window.__TAURI_INTERNALS__.invoke = async cmd => cmd === 'get_state' ? view : undefined;

  // Settings: signed in, linking an email.
  const signed = { signed_in: true, account: { id: 'acc', identities: [{ kind: 'telegram', display: '@te***', id: 'i1' }], vpn_source: 'telegram' }, email: { state: 'enter_email' } };
  await open('settings', signed, 'linkEmail');
  await textField('linkEmail', 'linkEmail', 'link email');
  ui.linkSent = true; ui.linkEmail = 'a@x.co'; render(); await wait(30);
  node = await start('linkCode');
  await typeChars('4242', 130);
  check('link code slow typing', field('linkCode').value, '4242');
  check('link code field never replaced', field('linkCode') === node, true);
  check('link code digits are visible, not masked', field('linkCode').type, 'text');

  await done({ lang: q.get('lang'), results });
})().catch(e => done({ crash: String(e), results: [] }));
