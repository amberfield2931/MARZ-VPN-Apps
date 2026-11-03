#!/bin/bash
# Builds sing-box v1.14.3 (the core of Android RC6) from source for linux or windows, stripped, with only the
# features the app uses. Usage: build-engine.sh <source dir> <linux|windows> <output file> [gocache dir]
set -eu
SRC=$1; OS=$2; OUT=$(realpath -m "$3")
[ -n "${4:-}" ] && export GOCACHE=$(realpath -m "$4")
cd "$SRC"
GOOS=$OS GOARCH=amd64 CGO_ENABLED=0 go build -trimpath -tags with_gvisor,with_utls,with_clash_api \
  -ldflags "-s -w -buildid= -checklinkname=0 -X github.com/sagernet/sing-box/constant.Version=1.14.3" -o "$OUT" ./cmd/sing-box
