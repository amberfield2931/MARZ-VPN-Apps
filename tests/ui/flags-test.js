// Appended to app.js by run-flag-tests.py, in the same scope. Reports what the servers screen really drew.
setTimeout(async () => {
  const rows = [...document.querySelectorAll('.option')];
  const out = {
    rows: rows.length,
    titles: rows.map(r => r.querySelector('.t-m')?.textContent ?? ''),
    pictures: rows.map(r => {
      const svg = r.querySelector('.flag svg');
      if (!svg) return null;
      const b = svg.getBoundingClientRect();
      return { w: Math.round(b.width), h: Math.round(b.height), neutral: svg.classList.contains('neutral'), fills: [...new Set([...svg.querySelectorAll('[fill]')].map(e => e.getAttribute('fill')))] };
    }),
    letters: rows.map(r => r.querySelector('.flag')?.textContent ?? ''),
    regional: /[\u{1F1E6}-\u{1F1FF}]/u.test(document.body.textContent),
    availability: document.querySelectorAll('.option .dot, .option [class*="avail"]').length,
    dir: document.documentElement.dir,
    remote: performance.getEntriesByType('resource').filter(e => !e.name.startsWith(location.origin)).map(e => e.name),
  };
  await fetch('/result', { method: 'POST', body: JSON.stringify(out) });
}, 500);
