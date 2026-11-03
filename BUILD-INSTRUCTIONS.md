# Build instructions

## Normal Linux build machine
```
sudo apt install build-essential pkg-config libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libdbus-1-dev
# Rust stable, Go 1.26, sing-box source at tag v1.14.3
tools/build-engine.sh <sing-box source> linux .build/sing-box-linux
cargo build --release -p marzvpn -p marz-helper
packaging/linux/build-deb.sh .build/sing-box-linux     # dist/marzvpn_<version>_amd64.deb, no root needed
```

## Windows installer, on Linux
```
packaging/windows/build-exe.sh    # dist/MARZ-VPN-<version>-windows-x64-setup.exe
```
Uses MinGW and NSIS unpacked with dpkg -x into a local directory (tools/win-env.sh). The installer is unsigned, so SmartScreen will warn; signing needs a certificate.

## Install

See the release notes.
