// Chromium driver for run-input-tests.py: real key events and an IME composition, then the in-page cases.
const [port, url] = process.argv.slice(2);
const page = (await (await fetch(`http://127.0.0.1:${port}/json/new?about:blank`, { method: 'PUT' })).json()).webSocketDebuggerUrl;
const ws = new WebSocket(page); await new Promise(r => ws.onopen = r);
let seq = 0; const pending = new Map();
ws.onmessage = m => { const d = JSON.parse(m.data); if (d.id && pending.has(d.id)) { pending.get(d.id)(d.result ?? d); pending.delete(d.id); } };
const cdp = (method, params = {}) => new Promise(r => { const id = ++seq; pending.set(id, r); ws.send(JSON.stringify({ id, method, params })); });
const wait = ms => new Promise(r => setTimeout(r, ms));
const js = async e => (await cdp('Runtime.evaluate', { expression: e, returnByValue: true })).result?.value;
const KEYS = { Backspace: 8, Delete: 46, ArrowLeft: 37, Home: 36, End: 35, Tab: 9 };
const key = async k => { for (const type of ['rawKeyDown', 'keyUp']) await cdp('Input.dispatchKeyEvent', { type, key: k, code: k, windowsVirtualKeyCode: KEYS[k] }); };
const char = async c => { await cdp('Input.dispatchKeyEvent', { type: 'keyDown', text: c, key: c, unmodifiedText: c }); await cdp('Input.dispatchKeyEvent', { type: 'keyUp', key: c }); };
const typeKeys = async (s, ms) => { for (const c of s) { await char(c); await wait(ms); } };
await cdp('Page.enable'); await cdp('Runtime.enable'); await cdp('Emulation.setFocusEmulationEnabled', { enabled: true });
// The in-page cases are held back until the real key events have run.
await cdp('Page.addScriptToEvaluateOnNewDocument', { source: 'window.__HOLD__ = true;' });
await cdp('Page.navigate', { url }); await wait(600);
const real = []; const check = (name, got, want) => real.push({ name: 'real keys: ' + name, ok: got === want, got: String(got).slice(0, 80), want: String(want).slice(0, 80) });
const ADDR = 'john.doe+tag_1-x@example-mail.co.uk', val = () => js("document.getElementById('email').value");
const clear = async () => { await js("(e=>{e.focus();e.select();document.execCommand('delete')})(document.getElementById('email'))"); await wait(20); };
await clear(); await typeKeys(ADDR, 130); check('slow typing', await val(), ADDR);
await clear(); await typeKeys(ADDR, 3); await wait(1150); check('fast typing', await val(), ADDR);
await clear(); await typeKeys('ab@x.co', 5); await wait(1150); await key('Backspace'); await wait(1150); check('Backspace', await val(), 'ab@x.c');
await key('ArrowLeft'); await key('ArrowLeft'); await wait(1150); await key('Delete'); await wait(30); check('Delete', await val(), 'ab@xc');
await clear(); await typeKeys('ab@x.co', 5); for (let i = 0; i < 5; i++) await key('ArrowLeft'); await wait(1150); await typeKeys('c', 5); await wait(1150); await typeKeys('d', 5); await wait(30);
check('edit in the middle', await val(), 'abcd@x.co');
await clear(); await cdp('Input.insertText', { text: ADDR }); await wait(1150); check('paste', await val(), ADDR);
// Keyboard layout: another layout simply types other characters; none may be lost or moved.
await clear(); await typeKeys('почта@пример.рф', 130); check('Cyrillic layout stays as typed', await val(), 'почта@пример.рф');
await clear(); await cdp('Input.imeSetComposition', { text: 'zh', selectionStart: 2, selectionEnd: 2 }); await wait(1150);
await cdp('Input.imeSetComposition', { text: 'zhong', selectionStart: 5, selectionEnd: 5 }); await wait(1150); await cdp('Input.insertText', { text: '中' }); await wait(30);
check('IME composition across re-renders', await val(), '中');
await clear(); await typeKeys('a@x', 5); await key('Tab'); await wait(1150); check('Tab away keeps the value', await val(), 'a@x');
const blur = await js("document.activeElement.id"); check('focus really moved', blur !== 'email', true);
await js('window.__GO__ && window.__GO__()');
const end = Date.now() + 140000;
while (Date.now() < end) { if (await js('document.title') === 'DONE') break; await wait(300); }
console.log(JSON.stringify(real)); ws.close(); process.exit(0);
