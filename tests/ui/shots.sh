#!/bin/bash
# Renders every screen with synthetic states. Usage: shots.sh <outdir> "<scenario[:lang[:theme[:WxH]]]> ..."
set -eu
HERE=$(cd "$(dirname "$0")" && pwd); UI=$HERE/../../app/ui; OUT=$1; mkdir -p "$OUT"
W=$(mktemp -d); cp -r "$UI"/. "$W"/; cp "$HERE/mock.js" "$W"/
sed -i 's#<script src="app.js"#<script src="mock.js"></script><script src="app.js"#' "$W/index.html"
python3 -I -m http.server 18790 --bind 127.0.0.1 --directory "$W" >/dev/null 2>&1 & SRV=$!
trap 'kill $SRV; rm -rf "$W"' EXIT
sleep 0.5
for item in $2; do
  IFS=: read -r s l t size <<< "$item"; l=${l:-ru}; t=${t:-light}; size=${size:-440x780}
  timeout 40 google-chrome --headless=new --disable-gpu --no-sandbox --hide-scrollbars --user-data-dir="$W/.prof" --window-size="${size/x/,}" --virtual-time-budget=1500 \
    --screenshot="$OUT/$s-$l-$t.png" "http://127.0.0.1:18790/index.html?s=$s&lang=$l&theme=$t" >/dev/null 2>&1 || echo "FAIL $item"
done
