# Build instructions

## Normal Linux build machine
```
sudo apt install build-essential pkg-config libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libdbus-1-dev
# Rust stable, Go 1.26, sing-box source at tag v1.14.3
tools/build-engine.sh <sing-box source> linux .build/sing-box-linux
cargo build --release -p marzvpn -p marz-helper
packaging/linux/build-deb.sh .build/sing-box-linux     # dist/marzvpn_<version>_amd64.deb, no root needed
```

## This Dell, without root or -dev packages
Everything is on /mnt/marz-lab/toolchains/desktop; nothing is installed in the system.
```
source tools/env.sh     # Rust, Go, pkg-config stubs from tools/stub-sysroot.sh
cargo build --release -p marzvpn -p marz-helper
packaging/linux/build-deb.sh .build/sing-box-linux
```
If the system resolver is broken, prefix a command with tools/with-private-dns.sh (private view of resolv.conf for that command only).
Disk is tight: Linux release 1.1 GB, Windows 0.8 GB, debug without the window 0.3 GB. Remove target/* between them.

## Windows installer, on Linux
```
packaging/windows/build-exe.sh    # dist/MARZ-VPN-<version>-windows-x64-setup.exe
```
Uses MinGW and NSIS unpacked with dpkg -x into /mnt/marz-lab/toolchains/desktop/mingw (tools/win-env.sh). The installer is unsigned, so SmartScreen will warn; signing needs a certificate.

## Tests
```
cargo test -p marz-engine -p marz-helper
cargo test -p marz-core --no-default-features
cargo build -p marz-helper; cargo build -p marz-core --no-default-features --features lab --bins
tests/api-lab/run.sh <backend worktree> target/debug/marz-lab-client <workdir>
tests/controller-lab/run.sh <backend worktree at the deployed RC6 commit, wt-rc6-sweden-integration> target/debug/marz-controller-lab target/debug/marzvpn-helper <workdir>
tests/ui/shots.sh <outdir> "home_on home_off:fa servers:ru:dark"
```
Kill switch and leak test on real kernels, in two throwaway VMs: tests/vm/README.md. Never run a real TUN on the Dell.

Regenerate texts from the RC6 tag: `python3 tools/gen_i18n.py <android repo> app/ui/i18n`.

## Install and remove (owner, needs a password)
```
sudo apt install /mnt/marz-lab/MARZ-VPN/wt-desktop-20261102/dist/marzvpn_0.1.2_amd64.deb
sudo apt remove marzvpn
```
