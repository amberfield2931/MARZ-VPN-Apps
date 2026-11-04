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
