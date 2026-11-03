# Third-party software in MARZ VPN Desktop

MARZ VPN Desktop is licensed under GPL-3.0-or-later (LICENSE).

| Component | Use | License |
|---|---|---|
| sing-box 1.14.3 (SagerNet) | VPN engine, a separate executable. Same core version as Android RC6. Linux: unmodified. Windows: `marzvpn-engine.exe`, modified on 2026-11-03 only to load `wintun.dll` from its own folder. | GPL-3.0-or-later, with the upstream's added name clause: derivatives may not use its name without consent. MARZ VPN does not use it as a product name. |
| Wintun 0.14.1 (WireGuard LLC), Windows only | Virtual network adapter. The official signed `wintun.dll`, unmodified (SHA-256 e5da8447…dafce, as in wintun-0.14.1.zip from wintun.net), installed next to the program and used only through the wintun.h API | Prebuilt Binaries License: `wintun-LICENSE.txt` and THIRD-PARTY-NOTICES.txt |
| WinDivert 2.2.2 (basil00), Windows only | Unmodified kernel driver embedded in the engine by sing-box, for a feature MARZ VPN does not use | LGPL-3.0 option of LGPL-3.0 / GPL-2.0: THIRD-PARTY-NOTICES.txt; source next to the installers |
| Tauri 2 | Application window | MIT / Apache-2.0 |
| rustls, rustls-webpki, ring | TLS | Apache-2.0 / ISC / MIT |
| RustCrypto p256, aes-gcm, sha2 | Device signatures, local encryption | MIT / Apache-2.0 |
| keyring-rs | OS keyring access | MIT / Apache-2.0 |
| Microsoft WebView2Loader.dll (Windows installer only) | Loads the system WebView2 | BSD-3-Clause text, Microsoft |
| Rust and Go standard libraries; mingw-w64 and GCC runtime parts (Windows); NSIS (Windows installer) | Linked into the builds, or the installer stub | MIT / Apache-2.0, BSD-3-Clause; ZPL, MIT, GCC Runtime Exception; zlib: full texts in THIRD-PARTY-NOTICES.txt |
| Roboto (via fontsource) | Interface font (app/ui/fonts) | OFL-1.1 / Apache-2.0: see app/ui/fonts/LICENSE |
| Material Icons (paths in app/ui/app.js) | Icons | Apache-2.0 |

Texts, colors and the logo come from MARZ VPN Android RC6 (tag marz-android-1.0.0-rc6-checkout-1000008).
The corresponding source of sing-box is its tag v1.14.3 at https://github.com/SagerNet/sing-box; the Windows change is in packaging/windows/wintun-side-by-side.
MARZ VPN is not affiliated with or endorsed by SagerNet, the sing-box project, WireGuard LLC or the Wintun project.
This program comes with ABSOLUTELY NO WARRANTY: see LICENSE.

Full list with license texts of every Rust crate and Go module in the builds: THIRD-PARTY-NOTICES.txt.
The complete corresponding source is published next to the installers.
