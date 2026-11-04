#!/bin/bash
# Cross-builds the Windows installer on Linux. OUT_TAG adds a tag to the output name for an internal build.
set -eu
ROOT=$(cd "$(dirname "$0")/../.." && pwd); cd "$ROOT"
source tools/env.sh; source tools/win-env.sh
cargo build --release --target x86_64-pc-windows-gnu -p marzvpn -p marz-helper
TUN=$(GOOS=windows go -C /mnt/marz-lab/MARZ-VPN-app/upstream/sing-box list -m -f '{{.Dir}}' github.com/sagernet/sing-tun)
packaging/windows/build-engine.sh /mnt/marz-lab/MARZ-VPN-app/upstream/sing-box "$TUN" .build/marzvpn-engine.exe
# The official signed DLL, unchanged. Its hash is that of wintun-0.14.1.zip from wintun.net.
install -m644 "$TUN/internal/wintun/amd64/wintun.dll" .build/wintun.dll
[ "$(sha256sum .build/wintun.dll | cut -d' ' -f1)" = e5da8447dc2c320edc0fc52fa01885c103de8c118481f683643cacc3220dafce ] || { echo "REFUSED: not the official wintun.dll"; exit 1; }
# The app imports WebView2Loader.dll at start; without it next to the exe it does not even open.
cp "$(find target/x86_64-pc-windows-gnu/release/build -path '*webview2-com-sys*/out/x64/WebView2Loader.dll' | head -1)" .build/WebView2Loader.dll
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
mkdir -p dist
# One folder with the final names: the installer packs it, the manifest vouches for it, the helper checks it on the
# target before anything is replaced.
PAYLOAD=$ROOT/.build/payload; DELETES=$ROOT/.build/payload-delete.nsh
mkdir -p "$PAYLOAD"
R=target/x86_64-pc-windows-gnu/release
while read -r src name; do install -m644 "$src" "$PAYLOAD/$name"; done <<E
$R/marzvpn.exe marzvpn.exe
$R/marzvpn-helper.exe marzvpn-helper.exe
.build/WebView2Loader.dll WebView2Loader.dll
.build/marzvpn-engine.exe marzvpn-engine.exe
.build/wintun.dll wintun.dll
tools/licenses/Wintun-Prebuilt-Binaries-LICENSE.txt wintun-LICENSE.txt
LICENSE LICENSE.txt
NOTICE.md NOTICE.md
RECOVERY-WINDOWS.txt RECOVERY.txt
tools/windows/marzvpn-wfp-recover.ps1 marzvpn-wfp-recover.ps1
THIRD-PARTY-NOTICES.txt THIRD-PARTY-NOTICES.txt
E
python3 -I packaging/windows/make-manifest.py "$PAYLOAD" "$DELETES"
python3 -I tests/windows/check-installer-script.py packaging/windows/installer.nsi "$PAYLOAD"
makensis -V2 -DROOT="$ROOT" -DVERSION="$VERSION" -DPAYLOAD="$PAYLOAD" -DPAYLOAD_DELETE="$DELETES" -DOUT="$ROOT/dist/MARZ-VPN-$VERSION-windows-x64${OUT_TAG:+-$OUT_TAG}-setup.exe" packaging/windows/installer.nsi
ls -la dist/
