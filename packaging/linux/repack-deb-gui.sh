#!/bin/bash
# A Linux package from the previous one with only the app binary rebuilt. Helper, engine, service files and maintainer
# scripts stay byte-identical, and the script refuses otherwise. 0.1.2-3 was the text field fix, 0.1.2-4 the drawn flags.
# 0.1.2-5: version shown in the app, source code link. The new version comes from packaging/linux/VERSION, the same file the app is built with.
# Usage: repack-deb-gui.sh <fixed app binary> <previous version>
set -eu
APP=$(realpath "$1"); FROM=$2; ROOT=$(cd "$(dirname "$0")/../.." && pwd); cd "$ROOT"
TO=$(cat packaging/linux/VERSION)
strings "$APP" | grep -qx -- "$TO" || strings "$APP" | grep -q -- "$TO" || { echo "REFUSED: the app was not built for $TO"; exit 1; }
if strings "$APP" | grep -q 'MARZ_API_URL'; then echo "REFUSED: lab build"; exit 1; fi
IN=dist/marzvpn_${FROM}_amd64.deb; OUT=dist/marzvpn_${TO}_amd64.deb; W=$(mktemp -d)
dpkg-deb -R "$IN" "$W/n"; dpkg-deb -R "$IN" "$W/o"
install -m755 "$APP" "$W/n/usr/bin/marzvpn"
sed -i "s/^Version: ${FROM}\$/Version: ${TO}/" "$W/n/DEBIAN/control"
chmod 755 "$W/n"
dpkg-deb --root-owner-group --build -Zxz "$W/n" "$OUT" | tail -1
rm -rf "$W/c"; dpkg-deb -R "$OUT" "$W/c"
diff -r --exclude=DEBIAN --exclude=marzvpn "$W/o" "$W/c" && echo "everything but the app is byte-identical"
diff <(cd "$W/o/DEBIAN"; cat prerm postinst postrm) <(cd "$W/c/DEBIAN"; cat prerm postinst postrm) && echo "same maintainer scripts"
diff "$W/o/DEBIAN/control" "$W/c/DEBIAN/control" || true
[ "$(dpkg-deb -c "$OUT" | awk '$NF=="./"{print $1; exit}')" = drwxr-xr-x ] || { echo "bad root mode"; rm -f "$OUT"; exit 1; }
(cd "$W/c/usr"; sha256sum bin/marzvpn lib/marzvpn/marzvpn-helper lib/marzvpn/sing-box)
sha256sum "$OUT"
