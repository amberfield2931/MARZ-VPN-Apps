# Source after env.sh: cross build for Windows with MinGW unpacked (dpkg -x, not installed) on the lab partition.
M=/mnt/marz-lab/toolchains/desktop/mingw
export PATH=$M/wrap:$M/root/usr/bin:$PATH
export CC_x86_64_pc_windows_gnu=$M/wrap/x86_64-w64-mingw32-gcc CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=$M/wrap/x86_64-w64-mingw32-gcc
export NSISDIR=$M/root/usr/share/nsis LD_LIBRARY_PATH=$M/root/usr/lib/x86_64-linux-gnu${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}
