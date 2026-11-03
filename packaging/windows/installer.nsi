; MARZ VPN for Windows. Builds an unsigned installer.
Unicode true
!include "MUI2.nsh"
!include "LogicLib.nsh"
Name "MARZ VPN"
OutFile "${OUT}"
InstallDir "$PROGRAMFILES64\MARZ VPN"
RequestExecutionLevel admin
SetCompressor /SOLID lzma
!define MUI_ICON "${ROOT}\app\src-tauri\icons\icon.ico"
!define MUI_UNICON "${ROOT}\app\src-tauri\icons\icon.ico"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "Russian"
!insertmacro MUI_LANGUAGE "English"
!define SVC "MarzVpnHelper"
!define UNINST "Software\Microsoft\Windows\CurrentVersion\Uninstall\MARZ VPN"

Function CheckWebView2
  ReadRegStr $0 HKLM "SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
  ${If} $0 == ""
    ReadRegStr $0 HKCU "Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
  ${EndIf}
  ${If} $0 == ""
    MessageBox MB_OK "MARZ VPN needs Microsoft Edge WebView2 Runtime. The download page will open; install it, then run MARZ VPN."
    ExecShell "open" "https://developer.microsoft.com/microsoft-edge/webview2/"
  ${EndIf}
FunctionEnd

Section "MARZ VPN"
  SetShellVarContext all
  nsExec::Exec 'taskkill /IM marzvpn.exe /F'
  nsExec::Exec 'sc stop ${SVC}'
  Sleep 1500
  SetOutPath "$INSTDIR"
  File "${ROOT}\target\x86_64-pc-windows-gnu\release\marzvpn.exe"
  File "${ROOT}\target\x86_64-pc-windows-gnu\release\marzvpn-helper.exe"
  File "${ROOT}\.build\WebView2Loader.dll"
  Delete "$INSTDIR\sing-box.exe"
  File "${ROOT}\.build\marzvpn-engine.exe"
  File "${ROOT}\.build\wintun.dll"
  File "/oname=wintun-LICENSE.txt" "${ROOT}\tools\licenses\Wintun-Prebuilt-Binaries-LICENSE.txt"
  File "/oname=LICENSE.txt" "${ROOT}\LICENSE"
  File "/oname=NOTICE.md" "${ROOT}\NOTICE.md"
  File "/oname=RECOVERY.txt" "${ROOT}\RECOVERY-WINDOWS.txt"
  File "/oname=marzvpn-wfp-recover.ps1" "${ROOT}\tools\windows\marzvpn-wfp-recover.ps1"
  File "/oname=THIRD-PARTY-NOTICES.txt" "${ROOT}\THIRD-PARTY-NOTICES.txt"
  nsExec::Exec 'sc delete ${SVC}'
  Sleep 500
  nsExec::ExecToLog 'sc create ${SVC} binPath= "\"$INSTDIR\marzvpn-helper.exe\" --service" start= auto depend= BFE DisplayName= "MARZ VPN helper"'
  nsExec::Exec 'sc failure ${SVC} reset= 60 actions= restart/1000/restart/1000/restart/5000'
  nsExec::Exec 'sc start ${SVC}'
  ; Holds only "the switch is ON": no credentials. Writable by SYSTEM and administrators alone.
  CreateDirectory "$APPDATA\MARZ VPN"
  nsExec::Exec 'icacls "$APPDATA\MARZ VPN" /inheritance:r /grant:r *S-1-5-18:(OI)(CI)F /grant:r *S-1-5-32-544:(OI)(CI)F'
  ; After a restart with the VPN left ON, only the app can bring the tunnel back. Exits at once otherwise.
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Run" "MARZ VPN resume" '"$INSTDIR\marzvpn.exe" --resume-if-armed'
  CreateShortCut "$SMPROGRAMS\MARZ VPN.lnk" "$INSTDIR\marzvpn.exe"
  CreateShortCut "$DESKTOP\MARZ VPN.lnk" "$INSTDIR\marzvpn.exe"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  WriteRegStr HKLM "${UNINST}" "DisplayName" "MARZ VPN"
  WriteRegStr HKLM "${UNINST}" "DisplayIcon" "$INSTDIR\marzvpn.exe"
  WriteRegStr HKLM "${UNINST}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKLM "${UNINST}" "Publisher" "MARZ VPN"
  Call CheckWebView2
SectionEnd

Section "Uninstall"
  SetShellVarContext all
  nsExec::Exec 'taskkill /IM marzvpn.exe /F'
  nsExec::Exec 'sc stop ${SVC}'
  Sleep 1500
  nsExec::Exec 'sc delete ${SVC}'
  ; Removing the app must give the internet back: nothing would be left to turn the protection off.
  nsExec::Exec '"$INSTDIR\marzvpn-helper.exe" --recover'
  DeleteRegValue HKLM "Software\Microsoft\Windows\CurrentVersion\Run" "MARZ VPN resume"
  RMDir /r "$APPDATA\MARZ VPN"
  Delete "$INSTDIR\marzvpn.exe"
  Delete "$INSTDIR\marzvpn-helper.exe"
  Delete "$INSTDIR\WebView2Loader.dll"
  Delete "$INSTDIR\marzvpn-engine.exe"
  Delete "$INSTDIR\sing-box.exe"
  Delete "$INSTDIR\wintun.dll"
  Delete "$INSTDIR\wintun-LICENSE.txt"
  Delete "$INSTDIR\LICENSE.txt"
  Delete "$INSTDIR\NOTICE.md"
  Delete "$INSTDIR\RECOVERY.txt"
  Delete "$INSTDIR\marzvpn-wfp-recover.ps1"
  Delete "$INSTDIR\THIRD-PARTY-NOTICES.txt"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\MARZ VPN.lnk"
  Delete "$DESKTOP\MARZ VPN.lnk"
  DeleteRegKey HKLM "${UNINST}"
SectionEnd
