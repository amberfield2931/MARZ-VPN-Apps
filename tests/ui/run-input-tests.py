#!/usr/bin/env python3
"""Text-field regression test on both engines the desktop app runs on: WebKitGTK (Linux) and Chromium (the engine of
WebView2 on Windows). Synthetic state only: no network, no sign-in, no letters.
Usage: run-input-tests.py <app/ui dir> [webkit|chrome|both]  [langs]   Exit code 1 if any check fails."""
import functools, http.server, json, os, shutil, socket, subprocess, sys, tempfile, threading, time, urllib.request
HERE = os.path.dirname(os.path.abspath(__file__)); UI = sys.argv[1]; WHICH = sys.argv[2] if len(sys.argv) > 2 else 'both'
RESULT = {}; LANGS = sys.argv[3].split(',') if len(sys.argv) > 3 else ['en', 'ru', 'fa', 'zh-CN']; TIMEOUT = 150

def site():
    w = tempfile.mkdtemp(prefix='marz-ui-'); d = os.path.join(w, 'ui'); shutil.copytree(UI, d)
    shutil.copy(os.path.join(HERE, 'mock.js'), d)
    # The test runs in the same scope as app.js, to see its state without any hook in the shipped file.
    with open(os.path.join(d, 'app.js'), 'a') as f: f.write('\n' + open(os.path.join(HERE, 'input-test.js')).read())
    p = os.path.join(d, 'index.html'); s = open(p).read()
    assert '<script src="app.js"' in s
    s = s.replace('<script src="app.js"', '<script src="mock.js"></script><script src="app.js"')
    open(p, 'w').write(s)
    class Q(http.server.SimpleHTTPRequestHandler):
        def log_message(self, *a): pass
        def do_POST(self):
            RESULT.update(json.loads(self.rfile.read(int(self.headers['Content-Length'])))); self.send_response(204); self.end_headers()
    srv = http.server.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(Q, directory=d))
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    return f'http://127.0.0.1:{srv.server_port}/index.html', w

def webkit(url):
    import gi
    gi.require_version('Gtk', '3.0'); gi.require_version('WebKit2', '4.1')
    from gi.repository import GLib, Gtk, WebKit2
    Gtk.init([]); win = Gtk.OffscreenWindow(); view = WebKit2.WebView(); win.add(view); win.set_default_size(440, 780); win.show_all()
    out = {}; loop = GLib.MainLoop(); end = time.time() + TIMEOUT
    def check():
        if RESULT: out.update(RESULT); loop.quit(); return False
        if time.time() > end: out['crash'] = 'timeout'; loop.quit(); return False
        return True
    view.load_uri(url); GLib.timeout_add(250, check); loop.run(); win.destroy()
    return out

def chrome(url):
    prof = tempfile.mkdtemp(prefix='marz-cr-')
    p = subprocess.Popen(['google-chrome', '--headless=new', '--disable-gpu', '--no-sandbox', '--no-first-run', '--disable-extensions', f'--user-data-dir={prof}', '--window-size=440,780', '--remote-debugging-port=0', 'about:blank'], stderr=subprocess.PIPE, text=True)
    try:
        ws = None
        for line in p.stderr:
            if 'DevTools listening on' in line: ws = line.split()[-1]; break
        port = ws.split('/')[2].split(':')[1]
        r = subprocess.run(['node', os.path.join(HERE, 'cdp-input.mjs'), port, url], capture_output=True, text=True, timeout=TIMEOUT + 60)
        if r.returncode != 0: return {'crash': r.stderr[-400:], 'results': []}
        o = dict(RESULT); o['results'] = json.loads(r.stdout) + o.get('results', [])
        return o
    finally:
        p.terminate(); p.wait(); shutil.rmtree(prof, ignore_errors=True)

url, w = site(); bad = 0
try:
    for name, run in (('webkit', webkit), ('chrome', chrome)):
        if WHICH not in (name, 'both'): continue
        for lang in LANGS:
            RESULT.clear(); o = run(f'{url}?s=email&lang={lang}')
            res = o.get('results', []); fails = [x for x in res if not x['ok']]
            print(f"{name:7} {lang:6} {len(res) - len(fails)} PASS, {len(fails)} FAIL" + (f"  CRASH {o['crash']}" if o.get('crash') else ''))
            for f in fails: print(f"   FAIL {f['name']}: got {f['got']!r}, want {f['want']!r}")
            bad += len(fails) + (1 if o.get('crash') or not res else 0)
finally:
    shutil.rmtree(w, ignore_errors=True)
print('RESULT', 'PASS' if not bad else f'FAIL ({bad})'); sys.exit(1 if bad else 0)
