#!/bin/bash
# Builds without root on a machine that has the Tauri runtime libraries but not their -dev packages
# (the -sys crates only link; they compile no C against the headers). Creates pkg-config files and .so links
# from the runtime libraries, under the given directory. On a normal build machine use the apt line in
# BUILD-INSTRUCTIONS.md instead.
set -eu
OUT=$1; L=/usr/lib/x86_64-linux-gnu
mkdir -p "$OUT/pkgconfig" "$OUT/lib"
pc() { # name version libname [so-file]
  local name=$1 ver=$2 lib=$3 so=${4:-}
  [ -n "$so" ] || so=$(ls "$L"/lib$lib.so.* 2>/dev/null | sort | head -1)
  [ -e "$so" ] || { echo "missing runtime library for $name" >&2; exit 1; }
  ln -sf "$so" "$OUT/lib/lib$lib.so"
  printf 'Name: %s\nDescription: runtime stub\nVersion: %s\nLibs: -L%s/lib -l%s\nCflags:\n' "$name" "$ver" "$OUT" "$lib" > "$OUT/pkgconfig/$name.pc"
}
pc glib-2.0 2.80.0 glib-2.0
pc gobject-2.0 2.80.0 gobject-2.0
pc gio-2.0 2.80.0 gio-2.0
pc gio-unix-2.0 2.80.0 gio-2.0
pc gmodule-2.0 2.80.0 gmodule-2.0
pc gdk-pixbuf-2.0 2.42.10 gdk_pixbuf-2.0
pc cairo 1.18.0 cairo
pc cairo-gobject 1.18.0 cairo-gobject
pc pango 1.52.1 pango-1.0
pc pangocairo 1.52.1 pangocairo-1.0
pc atk 2.52.0 atk-1.0
pc gtk+-3.0 3.24.41 gtk-3
pc gdk-3.0 3.24.41 gdk-3
pc gdk-x11-3.0 3.24.41 gdk-3
pc libsoup-3.0 3.4.4 soup-3.0
pc javascriptcoregtk-4.1 2.52.6 javascriptcoregtk-4.1
pc webkit2gtk-4.1 2.52.6 webkit2gtk-4.1
pc x11 1.8.7 X11
pc dbus-1 1.14.10 dbus-1
