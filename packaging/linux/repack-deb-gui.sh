#!/bin/bash
# A Linux package from the previous one with only the app binary rebuilt. Helper, engine, service files and maintainer
# scripts stay byte-identical, and the script refuses otherwise. 0.1.2-3 was the text field fix, 0.1.2-4 the drawn flags.
# Usage: repack-deb-gui.sh <fixed app binary> <previous revision, 2> <new revision, 3>, for 0.1.2-<revision>
set -eu
APP=$(realpath "$1"); FROM=$2; TO=$3; ROOT=$(cd "$(dirname "$0")/../.." && pwd); cd "$ROOT"
if strings "$APP" | grep -q 'MARZ_API_URL'; then echo "REFUSED: lab build"; exit 1; fi
IN=dist/marzvpn_0.1.2-${FROM}_amd64.deb; OUT=dist/marzvpn_0.1.2-${TO}_amd64.deb; W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
dpkg-deb -R "$IN" "$W/n"; dpkg-deb -R "$IN" "$W/o"
install -m755 "$APP" "$W/n/usr/bin/marzvpn"
sed -i "s/^Version: 0\.1\.2-${FROM}\$/Version: 0.1.2-${TO}/" "$W/n/DEBIAN/control"
chmod 755 "$W/n"
dpkg-deb --root-owner-group --build -Zxz "$W/n" "$OUT" | tail -1
rm -rf "$W/c"; dpkg-deb -R "$OUT" "$W/c"
diff -r --exclude=DEBIAN --exclude=marzvpn "$W/o" "$W/c" && echo "everything but the app is byte-identical"
diff <(cd "$W/o/DEBIAN"; cat prerm postinst postrm) <(cd "$W/c/DEBIAN"; cat prerm postinst postrm) && echo "same maintainer scripts"
diff "$W/o/DEBIAN/control" "$W/c/DEBIAN/control" || true
[ "$(dpkg-deb -c "$OUT" | awk '$NF=="./"{print $1; exit}')" = drwxr-xr-x ] || { echo "bad root mode"; rm -f "$OUT"; exit 1; }
(cd "$W/c/usr"; sha256sum bin/marzvpn lib/marzvpn/marzvpn-helper lib/marzvpn/sing-box)
sha256sum "$OUT"
