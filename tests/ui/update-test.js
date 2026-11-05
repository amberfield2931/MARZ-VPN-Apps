// Appended to app.js by run-update-tests.py, in the same scope. Presses the real buttons and reports which commands they sent.
setTimeout(async () => {
  const calls = [], real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = (cmd, args) => { if (cmd !== 'get_state') calls.push([cmd, args ?? null]); return real(cmd, args); };
  const texts = () => [...document.querySelectorAll('button')].map(b => ({ text: b.textContent.trim(), disabled: b.disabled }));
  const out = { text: document.body.textContent, buttons: texts(), dir: document.documentElement.dir, overflow: document.documentElement.scrollWidth > window.innerWidth + 1 };
  out.pressed = {};
  for (const label of (new URLSearchParams(location.search).get('press') || '').split('|').filter(Boolean)) {
    const b = [...document.querySelectorAll('button')].find(x => x.textContent.trim() === label && !x.disabled);
    calls.length = 0;
    if (b) { b.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true })); b.click(); b.dispatchEvent(new PointerEvent('pointerup', { bubbles: true })); }
    await new Promise(r => setTimeout(r, 150));
    out.pressed[label] = { found: !!b, calls: calls.slice(), after: document.body.textContent };
  }
  await fetch('/result', { method: 'POST', body: JSON.stringify(out) });
}, 900);
