; MARZ VPN for Windows. Builds an unsigned installer. Install and upgrade are one path: every new file is written and
; checked aside first, and then the helper (crates/marz-helper/src/install.rs) replaces the old version as one operation.
Unicode true
!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "FileFunc.nsh"
Name "MARZ VPN"
OutFile "${OUT}"
InstallDir "$PROGRAMFILES64\MARZ VPN"
RequestExecutionLevel admin
SetCompressor /SOLID lzma
; No file of this product may ever be skipped: "Ignore" is what left a new app with an old helper.
AllowSkipFiles off
!define MUI_ICON "${ROOT}\app\src-tauri\icons\icon.ico"
!define MUI_UNICON "${ROOT}\app\src-tauri\icons\icon.ico"
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipDirectoryOnUpgrade
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "Russian"
!insertmacro MUI_LANGUAGE "English"
!define SVC "MarzVpnHelper"
!define UNINST "Software\Microsoft\Windows\CurrentVersion\Uninstall\MARZ VPN"
!define TOOL ".update-tool"
!define STAGE ".update-new"

LangString TXT_VPN_ON ${LANG_RUSSIAN} "VPN сейчас включён. Для обновления MARZ VPN выключит его и снимет блокировку Kill Switch, и до повторного включения интернет будет работать напрямую, без VPN.$\n$\nПосле обновления включите VPN в приложении. Продолжить?"
LangString TXT_VPN_ON ${LANG_ENGLISH} "The VPN is on. To update, MARZ VPN will turn it off and lift the Kill Switch block, and your internet will go directly, without the VPN, until you turn it on again.$\n$\nTurn the VPN on in the app after the update. Continue?"
LangString TXT_SILENT_REFUSED ${LANG_RUSSIAN} "Тихое обновление отменено: VPN включён, и обновление отключило бы его незаметно. Ничего не изменено, VPN работает. Обновите без /S или добавьте /ALLOWVPNOFF."
LangString TXT_SILENT_REFUSED ${LANG_ENGLISH} "Silent update refused: the VPN is on and the update would turn it off unnoticed. Nothing was changed and the VPN keeps running. Update without /S, or add /ALLOWVPNOFF."
LangString TXT_RECONNECT ${LANG_RUSSIAN} "Обновление установлено. VPN сейчас выключен: включите его в приложении."
LangString TXT_RECONNECT ${LANG_ENGLISH} "The update is installed. The VPN is off now: turn it on in the app."
LangString TXT_CANCELLED ${LANG_RUSSIAN} "Обновление отменено, ничего не изменилось."
LangString TXT_CANCELLED ${LANG_ENGLISH} "The update was cancelled. Nothing was changed."
LangString TXT_STAGE ${LANG_RUSSIAN} "Не удалось записать файлы (возможно, нет места на диске). Установленная версия не изменилась."
LangString TXT_STAGE ${LANG_ENGLISH} "Could not write the files (the disk may be full). The installed version was not changed."
LangString TXT_STUCK ${LANG_RUSSIAN} "Не удалось остановить службу MARZ VPN. Файлы не изменялись. Перезагрузите компьютер и запустите установщик снова."
LangString TXT_STUCK ${LANG_ENGLISH} "Could not stop the MARZ VPN service. No files were changed. Restart the computer and run the installer again."
LangString TXT_FILTERS ${LANG_RUSSIAN} "Не удалось снять блокировку Kill Switch, поэтому обновление прервано, файлы не изменялись. Если интернет не работает, порядок действий описан в RECOVERY.txt в папке программы."
LangString TXT_FILTERS ${LANG_ENGLISH} "Could not lift the Kill Switch block, so the update stopped and no files were changed. If the internet does not work, follow RECOVERY.txt in the program folder."
LangString TXT_LOCKED ${LANG_RUSSIAN} "Файлы MARZ VPN заняты другой программой. Файлы не изменялись. Перезагрузите компьютер и запустите установщик снова."
LangString TXT_LOCKED ${LANG_ENGLISH} "Another program is holding the MARZ VPN files. No files were changed. Restart the computer and run the installer again."
LangString TXT_PENDING ${LANG_RUSSIAN} "Windows ещё удаляет старую службу MARZ VPN. Файлы не изменялись. Закройте окно Службы и Диспетчер задач или перезагрузите компьютер, затем запустите установщик снова. До этого VPN работать не будет."
LangString TXT_PENDING ${LANG_ENGLISH} "Windows is still removing the old MARZ VPN service. No files were changed. Close the Services window and Task Manager or restart the computer, then run the installer again. Until then the VPN will not work."
LangString TXT_ROLLED ${LANG_RUSSIAN} "Новая версия не запустилась, поэтому предыдущая версия MARZ VPN восстановлена. VPN выключен: его можно включить в приложении."
LangString TXT_ROLLED ${LANG_ENGLISH} "The new version did not start, so the previous version of MARZ VPN was restored. The VPN is off: you can turn it on in the app."
LangString TXT_BROKEN ${LANG_RUSSIAN} "Установить новую версию не удалось, а предыдущую не удалось восстановить полностью. Запустите установщик снова или удалите MARZ VPN и установите заново. Подробности: C:\ProgramData\MARZ VPN\install.log"
LangString TXT_BROKEN ${LANG_ENGLISH} "The new version could not be installed and the previous one could not be fully restored. Run the installer again, or uninstall MARZ VPN and install it anew. Details: C:\ProgramData\MARZ VPN\install.log"
LangString TXT_UN_STUCK ${LANG_RUSSIAN} "Не удалось остановить службу MARZ VPN или снять блокировку Kill Switch, поэтому удаление прервано. Перезагрузите компьютер и повторите. Порядок действий описан в RECOVERY.txt."
LangString TXT_UN_STUCK ${LANG_ENGLISH} "Could not stop the MARZ VPN service or lift the Kill Switch block, so the removal stopped. Restart the computer and try again. See RECOVERY.txt."

Var IsUpgrade
Var WasActive

Function .onInit
  ; An upgrade goes where the installed version is, wherever that is. Versions up to 0.1.5 stored only this value.
  StrCpy $IsUpgrade 0
  ReadRegStr $0 HKLM "${UNINST}" "UninstallString"
  ${If} $0 != ""
    Push $0
    Call DirFromUninstallString
    Pop $1
    ${If} $1 != ""
      StrCpy $INSTDIR $1
      StrCpy $IsUpgrade 1
    ${EndIf}
  ${EndIf}
FunctionEnd

; "C:\Program Files\MARZ VPN\uninstall.exe" -> C:\Program Files\MARZ VPN
Function DirFromUninstallString
  Exch $0
  Push $1
  Push $2
  StrCpy $1 $0 1
  ${If} $1 == '"'
    StrCpy $0 $0 "" 1
    StrCpy $0 $0 -1
  ${EndIf}
  StrLen $1 "\uninstall.exe"
  IntOp $1 0 - $1
  StrCpy $2 $0 "" $1
  ${If} $2 == "\uninstall.exe"
    StrCpy $0 $0 $1
  ${Else}
    StrCpy $0 ""
  ${EndIf}
  Pop $2
  Pop $1
  Exch $0
FunctionEnd

Function SkipDirectoryOnUpgrade
  ${If} $IsUpgrade == 1
    Abort
  ${EndIf}
FunctionEnd

Function CheckWebView2
  ReadRegStr $0 HKLM "SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
  ${If} $0 == ""
    ReadRegStr $0 HKCU "Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
  ${EndIf}
  ${If} $0 == ""
    MessageBox MB_OK "MARZ VPN needs Microsoft Edge WebView2 Runtime. The download page will open; install it, then run MARZ VPN." /SD IDOK
    IfSilent +2
    ExecShell "open" "https://developer.microsoft.com/microsoft-edge/webview2/"
  ${EndIf}
FunctionEnd

Function RemoveTool
  ; The current folder cannot be removed, and until the stage is written the installer's current folder is this one.
  SetOutPath "$TEMP"
  Delete "$INSTDIR\${TOOL}\marzvpn-helper.exe"
  RMDir "$INSTDIR\${TOOL}"
FunctionEnd

Function RemoveStage
  !include "${PAYLOAD_DELETE}"
  Delete "$INSTDIR\${STAGE}\install-manifest.txt"
  RMDir "$INSTDIR\${STAGE}"
FunctionEnd

; A cancelled or failed first install leaves no empty folder behind. A folder that holds anything stays.
Function GiveUp
  Pop $0
  MessageBox MB_OK|MB_ICONSTOP "$0" /SD IDOK
  DetailPrint "$0"
  Call RemoveTool
  Call RemoveStage
  RMDir "$INSTDIR"
  SetErrorLevel 2
  Abort "$0"
FunctionEnd

Section "MARZ VPN"
  SetShellVarContext all
  SetOverwrite on
  ; Holds "the switch is ON" and the install log, no credentials. Writable by SYSTEM and administrators alone.
  CreateDirectory "$APPDATA\MARZ VPN"
  nsExec::Exec 'icacls "$APPDATA\MARZ VPN" /inheritance:r /grant:r *S-1-5-18:(OI)(CI)F /grant:r *S-1-5-32-544:(OI)(CI)F'
  Pop $0

  ; The tool that does the work is the NEW helper, from a folder only administrators can write to.
  ClearErrors
  SetOutPath "$INSTDIR\${TOOL}"
  File "${PAYLOAD}\marzvpn-helper.exe"
  ${If} ${Errors}
    Push "$(TXT_STAGE)"
    Call GiveUp
  ${EndIf}

  ; Neither the installer nor the tool may sit in the folder that has to be removed afterwards.
  SetOutPath "$INSTDIR"
  StrCpy $WasActive 0
  nsExec::ExecToLog '"$INSTDIR\${TOOL}\marzvpn-helper.exe" --upgrade-inspect'
  Pop $0
  ; Anything but a clear "nothing is active" counts as active, a tool that could not even run included.
  ${If} $0 != 0
    StrCpy $WasActive 1
    ; A silent run has nobody to warn: it must not leave a person on direct internet unnoticed, unless whoever runs it says so.
    IfSilent 0 ask
      ${GetParameters} $1
      ClearErrors
      ${GetOptions} $1 "/ALLOWVPNOFF" $2
      ${If} ${Errors}
        DetailPrint "$(TXT_SILENT_REFUSED)"
        Call RemoveTool
        RMDir "$INSTDIR"
        SetErrorLevel 3
        Abort "$(TXT_SILENT_REFUSED)"
      ${EndIf}
      Goto proceed
    ask:
    MessageBox MB_YESNO|MB_ICONEXCLAMATION "$(TXT_VPN_ON)" IDYES proceed
    Push "$(TXT_CANCELLED)"
    Call GiveUp
    proceed:
  ${EndIf}

  ; Everything new is written aside while the old version still runs: a full disk changes nothing.
  ClearErrors
  SetOutPath "$INSTDIR\${STAGE}"
  File "${PAYLOAD}\*"
  ${If} ${Errors}
    Push "$(TXT_STAGE)"
    Call GiveUp
  ${EndIf}

  SetOutPath "$INSTDIR"
  nsExec::ExecToLog '"$INSTDIR\${TOOL}\marzvpn-helper.exe" --install-from-stage "$INSTDIR"'
  Pop $0
  ${If} $0 != 0
    ${If} $0 == 20
      StrCpy $1 "$(TXT_STAGE)"
    ${ElseIf} $0 == 21
      StrCpy $1 "$(TXT_STUCK)"
    ${ElseIf} $0 == 22
      StrCpy $1 "$(TXT_FILTERS)"
    ${ElseIf} $0 == 23
      StrCpy $1 "$(TXT_LOCKED)"
    ${ElseIf} $0 == 24
      StrCpy $1 "$(TXT_PENDING)"
    ${ElseIf} $0 == 25
      StrCpy $1 "$(TXT_ROLLED)"
    ${Else}
      StrCpy $1 "$(TXT_BROKEN)"
    ${EndIf}
    Push $1
    Call GiveUp
  ${EndIf}
  Call RemoveTool

  ; After a restart with the VPN left ON, only the app can bring the tunnel back. Exits at once otherwise.
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Run" "MARZ VPN resume" '"$INSTDIR\marzvpn.exe" --resume-if-armed'
  CreateShortCut "$SMPROGRAMS\MARZ VPN.lnk" "$INSTDIR\marzvpn.exe"
  CreateShortCut "$DESKTOP\MARZ VPN.lnk" "$INSTDIR\marzvpn.exe"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  WriteRegStr HKLM "${UNINST}" "DisplayName" "MARZ VPN"
  WriteRegStr HKLM "${UNINST}" "DisplayIcon" "$INSTDIR\marzvpn.exe"
  WriteRegStr HKLM "${UNINST}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKLM "${UNINST}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKLM "${UNINST}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKLM "${UNINST}" "Publisher" "MARZ VPN"
  Call CheckWebView2
  ${If} $WasActive == 1
    ; Through Explorer, so the app does not inherit the installer's administrator rights.
    MessageBox MB_OK|MB_ICONINFORMATION "$(TXT_RECONNECT)" /SD IDOK
    IfSilent +2
    Exec '"$WINDIR\explorer.exe" "$INSTDIR\marzvpn.exe"'
  ${EndIf}
SectionEnd

Section "Uninstall"
  SetShellVarContext all
  ; Removing the app must give the internet back: nothing would be left to turn the protection off.
  nsExec::ExecToLog '"$INSTDIR\marzvpn-helper.exe" --remove-service "$INSTDIR"'
  Pop $0
  ${If} $0 == "error"
    ; No helper file: a damaged install. The service, if any, can still be removed by name.
    nsExec::Exec 'taskkill /IM marzvpn.exe /F'
    nsExec::Exec 'sc failure ${SVC} reset= 0 actions= ""'
    nsExec::Exec 'sc stop ${SVC}'
    Sleep 3000
    nsExec::Exec 'sc delete ${SVC}'
  ${ElseIf} $0 != 0
    MessageBox MB_OK|MB_ICONSTOP "$(TXT_UN_STUCK)" /SD IDOK
    SetErrorLevel 2
    Abort "$(TXT_UN_STUCK)"
  ${EndIf}
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
  ; Only what an interrupted update could have left. Anything else in the folder stays, and so does the folder.
  RMDir /r "$INSTDIR\${TOOL}"
  RMDir /r "$INSTDIR\${STAGE}"
  RMDir /r "$INSTDIR\.update-old"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\MARZ VPN.lnk"
  Delete "$DESKTOP\MARZ VPN.lnk"
  DeleteRegKey HKLM "${UNINST}"
SectionEnd
