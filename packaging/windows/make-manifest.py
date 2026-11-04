#!/usr/bin/env python3
"""Writes install-manifest.txt (`<sha256>  <size>  <name>`, read by crates/marz-helper/src/install.rs) into the payload,
and the NSIS lines that delete those files from the stage. Usage: make-manifest.py <payload dir> <output .nsh>"""
import hashlib, os, sys
payload, nsh = sys.argv[1:3]
MANIFEST = 'install-manifest.txt'
names = sorted(n for n in os.listdir(payload) if n != MANIFEST)
with open(os.path.join(payload, MANIFEST), 'w', newline='\n') as f:
    for n in names:
        data = open(os.path.join(payload, n), 'rb').read()
        f.write(f'{hashlib.sha256(data).hexdigest()}  {len(data)}  {n}\n')
with open(nsh, 'w') as f:
    for n in names:
        f.write(f'Delete "$INSTDIR\\${{STAGE}}\\{n}"\n')
print(len(names), 'files in the manifest')
