#!/usr/bin/env python3
"""Country flags on both engines the desktop app runs on: WebKitGTK (Linux) and Chromium (the engine of WebView2 on Windows,
and like Windows it has no flag emoji to fall back on here: drawings do not depend on any font).
Synthetic state only. Usage: run-flag-tests.py <app/ui dir> [webkit|chrome|both]. Exit code 1 if any check fails."""
import functools, http.server, json, os, shutil, subprocess, sys, tempfile, threading, time
HERE = os.path.dirname(os.path.abspath(__file__)); UI = sys.argv[1]; WHICH = sys.argv[2] if len(sys.argv) > 2 else 'both'
RESULT = {}; TIMEOUT = 25; LANGS = ['en', 'ru', 'fa', 'zh-CN']
EXPECT_ORDER = {}

def site():
    w = tempfile.mkdtemp(prefix='marz-flags-'); d = os.path.join(w, 'ui'); shutil.copytree(UI, d); shutil.copy(os.path.join(HERE, 'mock.js'), d)
    with open(os.path.join(d, 'app.js'), 'a') as f: f.write('\n' + open(os.path.join(HERE, 'flags-test.js')).read())
    p = os.path.join(d, 'index.html'); s = open(p).read(); s = s.replace('<script src="app.js"', '<script src="mock.js"></script><script src="app.js"'); open(p, 'w').write(s)
    class Q(http.server.SimpleHTTPRequestHandler):
        def log_message(self, *a): pass
        def do_POST(self):
            RESULT.update(json.loads(self.rfile.read(int(self.headers['Content-Length'])))); self.send_response(204); self.end_headers()
    srv = http.server.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(Q, directory=d))
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    return srv.server_port, w

def wait():
    end = time.time() + TIMEOUT
    while time.time() < end and not RESULT: time.sleep(.1)

def chrome(url):
    prof = tempfile.mkdtemp(prefix='marz-cr-')
    p = subprocess.Popen(['google-chrome', '--headless=new', '--disable-gpu', '--no-sandbox', '--no-first-run', '--disable-extensions', f'--user-data-dir={prof}', '--window-size=440,780', url], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try: wait()
    finally: p.terminate(); p.wait(); shutil.rmtree(prof, ignore_errors=True)

def webkit(url):
    import gi
    gi.require_version('Gtk', '3.0'); gi.require_version('WebKit2', '4.1')
    from gi.repository import GLib, Gtk, WebKit2
    Gtk.init([]); win = Gtk.OffscreenWindow(); view = WebKit2.WebView(); win.add(view); win.set_default_size(440, 780); win.show_all()
    loop = GLib.MainLoop(); end = time.time() + TIMEOUT
    def check():
        if RESULT or time.time() > end: loop.quit(); return False
        return True
    view.load_uri(url); GLib.timeout_add(100, check); loop.run(); win.destroy()

port, w = site(); bad = 0
def check(name, ok, info=''):
    global bad
    print(('ok   ' if ok else 'FAIL ') + name + (f'  {info}' if not ok and info else '')); bad += not ok
try:
    for engine, run in (('webkit', webkit), ('chrome', chrome)):
        if WHICH not in (engine, 'both'): continue
        for lang in LANGS:
            for scene, expected, neutral_rows in (('servers', 6, 1), ('servers_unknown', 8, 3)):
                RESULT.clear(); run(f'http://127.0.0.1:{port}/index.html?s={scene}&lang={lang}')
                r = dict(RESULT); tag = f'{engine} {lang} {scene}'
                if not r: check(tag + ' reported', False); continue
                pics = r['pictures']
                check(tag + ': every row has a picture', r['rows'] == expected and all(pics), r)
                check(tag + ': no letters instead of a flag', all(not x.strip() for x in r['letters']), r['letters'])
                check(tag + ': no flag emoji anywhere', not r['regional'])
                check(tag + ': one picture size for all flags', len({(p['w'], p['h']) for p in pics if p and not p['neutral']}) == 1 and all(p['w'] >= 24 for p in pics if p), pics)
                check(tag + ': auto and unknown use the neutral icon only', sum(1 for p in pics if p and p['neutral']) == neutral_rows, pics)
                check(tag + ': nothing loaded from outside', not r['remote'], r['remote'])
                key = (lang, scene)
                if engine == 'webkit': EXPECT_ORDER[key] = r['titles']
                elif key in EXPECT_ORDER: check(tag + ': same titles and order as on the other engine', EXPECT_ORDER[key] == r['titles'], r['titles'])
finally:
    shutil.rmtree(w, ignore_errors=True)
print('FAILED' if bad else 'flag tests passed'); sys.exit(1 if bad else 0)
