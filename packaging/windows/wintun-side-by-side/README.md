# Wintun side by side

Upstream sing-tun embeds wintun.dll in the executable. The Wintun license allows redistribution when the DLL is "distributed alongside other software that uses the Software only via the Permitted API", and the Wintun documentation says to install it side by side with the application. So this build ships the official, unmodified, signed DLL next to the engine.

`build-engine.sh` builds with a local copy of `github.com/sagernet/sing-tun` in which `dll_windows.go` and `empty_embed.go` replace two files of `internal/wintun`, and the embedded DLLs and their in-memory loader are removed. No network code changes. Because the result is a modified sing-box, under GPL-3.0-or-later and its name clause it is installed as `marzvpn-engine.exe`.
