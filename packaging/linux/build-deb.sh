#!/bin/bash
# Builds dist/marzvpn_<version>_amd64.deb without root and without installing anything.
# Usage: build-deb.sh <sing-box binary>
set -eu
ROOT=$(cd "$(dirname "$0")/../.." && pwd); ENGINE=$(realpath "$1"); cd "$ROOT"
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
APP=target/release/marzvpn; HELPER=target/release/marzvpn-helper
[ -x "$APP" ] && [ -x "$HELPER" ] || { echo "build release binaries first"; exit 1; }
# A test build must never be packaged.
if strings "$APP" | grep -q 'MARZ_API_URL'; then echo "REFUSED: $APP is a lab build"; exit 1; fi
P=$(mktemp -d); trap 'rm -rf "$P"' EXIT
chmod 755 "$P" # mktemp makes 0700, and the package's "./" entry must never carry that
install -Dm755 "$APP" "$P/usr/bin/marzvpn"
install -Dm755 "$HELPER" "$P/usr/lib/marzvpn/marzvpn-helper"
install -Dm755 "$ENGINE" "$P/usr/lib/marzvpn/sing-box"
install -Dm644 packaging/linux/marzvpn-helper.service "$P/usr/lib/systemd/system/marzvpn-helper.service"
install -Dm644 packaging/linux/marzvpn.desktop "$P/usr/share/applications/marzvpn.desktop"
install -Dm644 packaging/linux/marzvpn-resume.desktop "$P/etc/xdg/autostart/marzvpn-resume.desktop"
install -Dm644 RECOVERY-LINUX.txt "$P/usr/share/doc/marzvpn/RECOVERY.txt"
for n in 16 22 24 32 48 64 128 256 512; do install -Dm644 "app/src-tauri/icons/${n}x${n}.png" "$P/usr/share/icons/hicolor/${n}x${n}/apps/marzvpn.png"; done
install -Dm644 app/ui/logo.svg "$P/usr/share/icons/hicolor/scalable/apps/marzvpn.svg"
install -Dm644 packaging/linux/com.marzvpn.helper.policy "$P/usr/share/polkit-1/actions/com.marzvpn.helper.policy"
install -Dm644 LICENSE "$P/usr/share/doc/marzvpn/copyright"
install -Dm644 NOTICE.md "$P/usr/share/doc/marzvpn/NOTICE.md"
mkdir -p "$P/DEBIAN"
SIZE=$(du -sk "$P/usr" | cut -f1)
cat > "$P/DEBIAN/control" <<C
Package: marzvpn
Version: $VERSION
Architecture: amd64
Maintainer: MARZ VPN <support@marzvpn.com>
Installed-Size: $SIZE
Depends: libwebkit2gtk-4.1-0, libgtk-3-0t64 | libgtk-3-0, libsoup-3.0-0, libdbus-1-3, iproute2, nftables, xdg-utils, gnome-keyring | kwalletmanager | keepassxc
Recommends: pkexec | policykit-1
Section: net
Priority: optional
Homepage: https://marzvpn.com
Description: MARZ VPN for Linux
 Connect to MARZ VPN with one button. Uses your existing MARZ VPN account.
C
cat > "$P/DEBIAN/postinst" <<'C'
#!/bin/sh
set -e
if [ "$1" = configure ] && [ -d /run/systemd/system ]; then
  systemctl daemon-reload
  # An older unit was wanted by multi-user.target: this also moves the link to sysinit.target.
  systemctl disable marzvpn-helper.service >/dev/null 2>&1 || true
  systemctl enable marzvpn-helper.service || true
  # Upgrade: the new helper reads the saved state, so a switch left ON stays ON and blocking.
  systemctl restart marzvpn-helper.service || true
fi
C
cat > "$P/DEBIAN/prerm" <<'C'
#!/bin/sh
set -e
# Removing the app must give the internet back: nothing would be left to turn the protection off.
# An upgrade keeps it (the postinst restarts the new helper).
if [ "$1" = remove ] || [ "$1" = purge ]; then
  if [ -d /run/systemd/system ]; then
    systemctl disable --now marzvpn-helper.service || true
  fi
  /usr/lib/marzvpn/marzvpn-helper --recover || true
fi
C
cat > "$P/DEBIAN/postrm" <<'C'
#!/bin/sh
set -e
[ -d /run/systemd/system ] && systemctl daemon-reload || true
if [ "$1" = purge ]; then rm -rf /var/lib/marzvpn; fi
C
chmod 755 "$P"/DEBIAN/postinst "$P"/DEBIAN/prerm "$P"/DEBIAN/postrm
mkdir -p dist
dpkg-deb --root-owner-group --build -Zxz "$P" "dist/marzvpn_${VERSION}_amd64.deb" | tail -1
ROOTMODE=$(dpkg-deb -c "dist/marzvpn_${VERSION}_amd64.deb" | awk '$NF=="./"{print $1; exit}')
[ "$ROOTMODE" = drwxr-xr-x ] || { echo "REFUSED: root entry mode is $ROOTMODE"; rm -f "dist/marzvpn_${VERSION}_amd64.deb"; exit 1; }
