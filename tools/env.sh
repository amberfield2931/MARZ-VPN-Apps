# Source this: toolchain on the lab partition, nothing installed in the system.
TC=/mnt/marz-lab/toolchains/desktop
export RUSTUP_HOME=$TC/rustup CARGO_HOME=$TC/cargo PATH=$TC/cargo/bin:/mnt/marz-lab/MARZ-VPN-app/toolchain/go/bin:$PATH
export GOPATH=/mnt/marz-lab/MARZ-VPN-app/toolchain/gopath GOCACHE=/mnt/marz-lab/MARZ-VPN-app/toolchain/gocache
if [ -d "$TC/sysroot/pkgconfig" ]; then
  export PKG_CONFIG_PATH=$TC/sysroot/pkgconfig PKG_CONFIG_ALLOW_SYSTEM_CFLAGS=1 PKG_CONFIG_ALLOW_SYSTEM_LIBS=1
fi
