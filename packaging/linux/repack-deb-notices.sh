#!/bin/bash
# Linux 0.1.2-2: the 0.1.2 app and engine byte for byte, the helper with the recovery fix, and the license files.
# Refuses unless every other file that existed before is byte-identical. Usage: repack-deb-notices.sh <fixed helper>
set -eu
HELPER=$(realpath "$1"); ROOT=$(cd "$(dirname "$0")/../.." && pwd); cd "$ROOT"
IN=dist/marzvpn_0.1.2_amd64.deb; OUT=dist/marzvpn_0.1.2-2_amd64.deb; W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
dpkg-deb -R "$IN" "$W/n"; dpkg-deb -R "$IN" "$W/o"
install -m755 "$HELPER" "$W/n/usr/lib/marzvpn/marzvpn-helper"
install -m644 THIRD-PARTY-NOTICES.txt "$W/n/usr/share/doc/marzvpn/THIRD-PARTY-NOTICES.txt"
install -m644 NOTICE.md "$W/n/usr/share/doc/marzvpn/NOTICE.md"
sed -i 's/^Version: 0\.1\.2$/Version: 0.1.2-2/' "$W/n/DEBIAN/control"
chmod 755 "$W/n"
dpkg-deb --root-owner-group --build -Zxz "$W/n" "$OUT" | tail -1
rm -rf "$W/c"; dpkg-deb -R "$OUT" "$W/c"
diff -r --exclude=THIRD-PARTY-NOTICES.txt --exclude=NOTICE.md --exclude=DEBIAN --exclude=marzvpn-helper "$W/o" "$W/c" && echo "same files except the helper and the notices"
diff <(cd "$W/o/DEBIAN"; cat prerm postinst postrm) <(cd "$W/c/DEBIAN"; cat prerm postinst postrm) && echo "same maintainer scripts"
diff "$W/o/DEBIAN/control" "$W/c/DEBIAN/control" || true
(cd "$W/c/usr"; sha256sum bin/marzvpn lib/marzvpn/marzvpn-helper lib/marzvpn/sing-box)
[ "$(dpkg-deb -c "$OUT" | awk '$NF=="./"{print $1; exit}')" = drwxr-xr-x ] || { echo "bad root mode"; rm -f "$OUT"; exit 1; }
