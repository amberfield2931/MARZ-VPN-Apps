#!/usr/bin/env python3
"""GOPROXY=file:// tree of every module the desktop engine needs, Linux and Windows, from the local module cache.
Refuses to finish if anything is missing. Usage: make-go-proxy.py <sing-box source> <output dir>"""
import json, os, re, shutil, subprocess, sys
src, out = sys.argv[1], sys.argv[2]
cache = subprocess.run(['go', 'env', 'GOMODCACHE'], capture_output=True, text=True, check=True).stdout.strip() + '/cache/download'
enc = lambda s: re.sub(r'[A-Z]', lambda m: '!' + m.group().lower(), s)
run = lambda *a, **e: subprocess.run(a, cwd=src, env=dict(os.environ, CGO_ENABLED='0', GOARCH='amd64', **e), capture_output=True, text=True, check=True).stdout
needed = set()
for goos in ('linux', 'windows'):
    needed |= {x for x in run('go', 'list', '-deps', '-tags', 'with_gvisor,with_utls,with_clash_api', '-f', '{{if not .Standard}}{{with .Module}}{{.Path}}@{{.Version}}{{end}}{{end}}', './cmd/sing-box', GOOS=goos).split() if '@' in x and not x.endswith('@')}
graph = set()
for line in run('go', 'mod', 'graph').splitlines():
    graph |= {x for x in line.split() if '@' in x}
missing = []
def copy(path, ver, ext, must):
    rel = f'{enc(path)}/@v/{enc(ver)}{ext}'; s = f'{cache}/{rel}'
    if os.path.exists(s):
        os.makedirs(os.path.dirname(f'{out}/{rel}'), exist_ok=True); shutil.copy(s, f'{out}/{rel}')
    elif must:
        missing.append(rel)
# With module graph pruning the go command never reads the .mod of a pruned leaf, so those may be absent.
# A build from this tree is the real check: see the release checkpoint.
for m in graph:
    p, v = m.split('@'); copy(p, v, '.mod', False); copy(p, v, '.info', False)
for m in needed:
    p, v = m.split('@'); copy(p, v, '.mod', True); copy(p, v, '.zip', True); copy(p, v, '.info', False)
if missing:
    sys.exit('missing from the module cache: ' + ' '.join(missing[:12]))
print(f'go proxy: {len(needed)} modules with source, {len(graph)} in the graph')
