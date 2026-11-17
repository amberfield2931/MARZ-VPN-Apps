# MARZ VPN apps

Official client apps of [MARZ VPN](https://marzvpn.com) for Android, Windows and Linux.
Downloads are in [Releases](../../releases).

## Current versions — release [stable-2026-11-16](../../releases/tag/stable-2026-11-16)

| Platform | Version | Channel | File | SHA-256 |
|---|---|---|---|---|
| Android 7.0+ (arm64-v8a) | 1.0.3 (versionCode 1000390) | **Stable** | `MARZ-VPN-Android-1.0.3.apk` | `ca1983484f093edc940f1797ffdefe5a8828e6df179e25d6bc32bd8d1bfd1577` |
| Linux amd64 (.deb, systemd) | 0.1.2-8 | **Stable** | `MARZ-VPN-0.1.2-8-amd64.deb` | `62f5ae7fdd7b56d5d883884aa5057c50cdce682267f3213af2dc546d62a88796` |
| Windows 11 x64 | 0.1.9 | **Beta** | `MARZ-VPN-Setup-0.1.9.exe` | `c3d661d0c878b175bab8ee6b11797496d0626f002fa67e0fba9eaf53b01713b4` |

Windows 0.1.9 is a **Beta**: it passed automated tests and tests under Wine, but has not been run on
real Windows 11 yet, so its Kill Switch on real Windows is not yet confirmed. Windows 0.1.8 (Stable)
stays available in [stable-2026-11-05-2](../../releases/tag/stable-2026-11-05-2). The Windows installer
has no digital signature yet, so SmartScreen may warn you. The Android app has no Kill Switch of its own.
There is no MARZ VPN app for iPhone or macOS; the subscription works there in compatible third-party clients.

The same files are on [marzvpn.com](https://marzvpn.com) and on the official SourceForge mirror,
https://sourceforge.net/projects/marz-vpn/. Each release lists its SHA-256 sums in `SHA256SUMS.txt`.

## Installing and updating

Instructions: [Android](https://marzvpn.com/android), [Windows](https://marzvpn.com/windows),
[Linux](https://marzvpn.com/linux).

- **Android:** install the APK. A new version installs over the old one and keeps your sign-in and
  settings (all releases are signed with the same certificate,
  SHA-256 `341f9b40054fafe26ce3afd2cd633f727d0aef1045d641b96c4c59990a10ff6a`).
- **Linux:** `sudo apt install ./MARZ-VPN-0.1.2-8-amd64.deb` installs or updates. If you ever lose the
  internet after a crash: `sudo /usr/lib/marzvpn/marzvpn-helper --recover` (`RECOVERY-LINUX.txt`).
- **Windows:** run the installer; it updates an existing install. Recovery: `RECOVERY-WINDOWS.txt`.

The apps look for new versions only at `https://marzvpn.com/updates/manifest.json` (at most once a day,
or from the button in Settings). They show a notice and never download or install anything by themselves.

## Source code

All apps are free software under **GPL-3.0-or-later** and come with ABSOLUTELY NO WARRANTY.

| What | Where |
|---|---|
| **Complete corresponding source** of every published build | the `*-corresponding-source.tar` files in each release (also on marzvpn.com and SourceForge). They hold the app source, the sing-box engine and every Go module it links, the Rust crates, the LGPL sources (WinDivert), license texts and build instructions |
| MARZ VPN Desktop (Windows and Linux): app, helper, engine overlay, packaging | this repository. `main` and tag [`desktop-0.1.9-linux-0.1.2-8`](../../tree/desktop-0.1.9-linux-0.1.2-8) are the source of Windows 0.1.9 and of the Linux 0.1.2-8 app |
| MARZ VPN for Android | [MARZ-VPN-Android](https://github.com/amberfield2931/MARZ-VPN-Android) (`main` = 1.0.3, one tag per version) |

Notes on the desktop source:
- The `main` tree is the app part of `MARZ-VPN-Desktop-0.1.2-8-0.1.9-corresponding-source.tar`, unchanged
  (plus this README).
- The Linux package 0.1.2-8 carries the helper `marzvpn-helper` built from the 0.1.2-2 helper tree. That
  helper has been byte-identical since 0.1.2-2. Its source is `marzvpn-desktop-0.1.2-2-linux-helper.tar.gz`
  inside the same tar; it is not the helper code on `main`.
- The git tag `stable-2026-11-16` points to an earlier documentation commit whose tree is still the
  0.1.8 / 0.1.2-7 source. The source of the stable-2026-11-16 builds is the tar in that release and the
  commit tagged `desktop-0.1.9-linux-0.1.2-8`.
- Build instructions: [`BUILD-INSTRUCTIONS.md`](BUILD-INSTRUCTIONS.md) and `README.txt` in the tar.

Third-party components and their licenses: [`NOTICE.md`](NOTICE.md),
[`THIRD-PARTY-NOTICES.txt`](THIRD-PARTY-NOTICES.txt). The Android app is a modified version of
sing-box-for-android by nekohasekai. The Windows app uses Wintun (prebuilt, signed DLL under its own
license) and the WinDivert driver (LGPL-3.0). MARZ VPN is not affiliated with or endorsed by SagerNet,
the sing-box project, WireGuard LLC or the Wintun project.

The server side of the service is not part of these apps.
