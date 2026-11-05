#!/usr/bin/env python3
"""The update check in the window, on both engines the desktop app runs on (WebKitGTK on Linux, Chromium as for WebView2 on Windows), in all four languages.
Synthetic state only, no network. Usage: run-update-tests.py <app/ui dir> [webkit|chrome|both]. Exit code 1 if any check fails."""
import functools, http.server, json, os, shutil, subprocess, sys, tempfile, threading, time, urllib.parse
HERE = os.path.dirname(os.path.abspath(__file__)); UI = sys.argv[1]; WHICH = sys.argv[2] if len(sys.argv) > 2 else 'both'
RESULT = {}; TIMEOUT = 25; LANGS = {'en': '', 'ru': '/ru', 'fa': '/fa', 'zh-CN': '/zh-cn'}

def site():
    w = tempfile.mkdtemp(prefix='marz-update-'); d = os.path.join(w, 'ui'); shutil.copytree(UI, d); shutil.copy(os.path.join(HERE, 'mock.js'), d)
    with open(os.path.join(d, 'app.js'), 'a') as f: f.write('\n' + open(os.path.join(HERE, 'update-test.js')).read())
    p = os.path.join(d, 'index.html'); s = open(p).read(); s = s.replace('<script src="app.js"', '<script src="mock.js"></script><script src="app.js"'); open(p, 'w').write(s)
    class Q(http.server.SimpleHTTPRequestHandler):
        def log_message(self, *a): pass
        def do_POST(self):
            RESULT.update(json.loads(self.rfile.read(int(self.headers['Content-Length'])))); self.send_response(204); self.end_headers()
    srv = http.server.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(Q, directory=d))
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    return srv.server_port, w

def chrome(url):
    prof = tempfile.mkdtemp(prefix='marz-cr-')
    p = subprocess.Popen(['google-chrome', '--headless=new', '--disable-gpu', '--no-sandbox', '--no-first-run', '--disable-extensions', '--window-size=440,780', f'--user-data-dir={prof}', url], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        end = time.time() + TIMEOUT
        while time.time() < end and not RESULT: time.sleep(.1)
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
strip = lambda s: s.replace('⁦', '').replace('⁩', '')
try:
    for engine, run in (('webkit', webkit), ('chrome', chrome)):
        if WHICH not in (engine, 'both'): continue
        for lang, prefix in LANGS.items():
            T = json.load(open(os.path.join(UI, 'i18n', lang + '.json'), encoding='utf8'))
            avail = T['update_available'].replace('%1$s', '0.1.2-7')
            def load(scene, press=()):
                RESULT.clear(); run(f"http://127.0.0.1:{port}/index.html?" + urllib.parse.urlencode({'s': scene, 'lang': lang, 'press': '|'.join(press)}))
                return dict(RESULT)
            tag = f'{engine} {lang}'
            r = load('settings', [T['update_check']])
            if not r: check(tag + ' reported', False); continue
            check(tag + ': version shown', T['version'].split('%')[0].strip() in r['text'])
            check(tag + ': page direction', r['dir'] == ('rtl' if lang == 'fa' else 'ltr'), r['dir'])
            p = r['pressed'].get(T['update_check'], {})
            check(tag + ': the button asks for one check', p.get('calls') == [['check_updates', None]], p.get('calls'))
            r = load('settings_update_checking')
            check(tag + ': checking, button off', T['update_checking'] in r['text'] and any(b['text'] == T['update_check'] and b['disabled'] for b in r['buttons']), r['buttons'])
            r = load('settings_update_latest', [T['update_check']])
            check(tag + ': latest', T['update_latest'] in r['text'] and r['pressed'][T['update_check']]['calls'] == [['check_updates', None]])
            r = load('settings_update_failed', [T['update_retry']])
            check(tag + ': failed, with retry', T['update_failed'] in r['text'] and r['pressed'][T['update_retry']]['calls'] == [['check_updates', None]])
            r = load('settings_update_available', [T['update_download']])
            check(tag + ': new version with its number', avail in strip(r['text']))
            check(tag + ': the version number is isolated left-to-right', '\u2066' + '0.1.2-7' + '\u2069' in r['text'])
            calls = r['pressed'][T['update_download']]['calls']
            ok = len(calls) == 1 and calls[0][0] == 'open_external' and calls[0][1]['url'] in {f'https://marzvpn.com{prefix}/linux', f'https://marzvpn.com{prefix}/windows'}
            check(tag + ': download only opens the official page', ok, calls)
            r = load('home_update', [T['update_later']])
            check(tag + ': home shows the notice, VPN controls stay', avail in strip(r['text']) and not r['overflow'])
            p = r['pressed'][T['update_later']]
            check(tag + ': later hides it and sends nothing', p['calls'] == [] and avail not in strip(p['after']), p['calls'])
            r = load('home_off')
            check(tag + ': nothing shown with no update', avail not in strip(r['text']))
finally:
    shutil.rmtree(w, ignore_errors=True)
print('FAILED' if bad else 'update UI tests passed'); sys.exit(1 if bad else 0)
