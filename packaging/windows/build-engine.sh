#!/bin/bash
# Windows engine: sing-box v1.14.3 with a local copy of sing-tun in which only the two files of wintun-side-by-side
# differ. Refuses a result that still carries a DLL. Usage: build-engine.sh <sing-box source> <sing-tun module dir> <output>
set -eu
HERE=$(cd "$(dirname "$0")" && pwd); SRC=$(realpath "$1"); TUN=$(realpath "$2"); OUT=$(realpath -m "$3"); W=/tmp/marzvpn-engine-build; [ ! -e "$W" ] || { chmod -R u+w "$W"; rm -rf "$W"; }; mkdir -p "$W"; trap 'chmod -R u+w "$W"; rm -rf "$W"' EXIT  # fixed path: it is recorded in the build info
cp -r "$TUN" "$W/sing-tun"; chmod -R u+w "$W/sing-tun"; I="$W/sing-tun/internal/wintun"
cp "$HERE/wintun-side-by-side/dll_windows.go" "$I/dll_windows.go"
cp "$HERE/wintun-side-by-side/empty_embed.go" "$I/dll_windows_amd64.go"
rm -rf "$I"/amd64 "$I"/arm64 "$I"/arm "$I"/x86 "$I"/memmod "$I"/dll_windows_386.go "$I"/dll_windows_arm.go "$I"/dll_windows_arm64.go
cp "$SRC/go.mod" "$W/build.mod"; cp "$SRC/go.sum" "$W/build.sum"
cd "$SRC"
go mod edit -modfile="$W/build.mod" -replace "github.com/sagernet/sing-tun=$W/sing-tun"
GOOS=windows GOARCH=amd64 CGO_ENABLED=0 go build -buildvcs=false -modfile="$W/build.mod" -trimpath -tags with_gvisor,with_utls,with_clash_api \
  -ldflags "-s -w -buildid= -checklinkname=0 -X github.com/sagernet/sing-box/constant.Version=1.14.3" -o "$OUT" ./cmd/sing-box
python3 - "$OUT" <<'P' || { echo "REFUSED: the engine still embeds a DLL"; rm -f "$OUT"; exit 1; }
import sys
d = open(sys.argv[1], 'rb').read()
sys.exit(1 if b'WireGuard LLC' in d and d.count(b'!This program cannot be run in DOS mode') > 1 else 0)
P
