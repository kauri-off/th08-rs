; th08 installer template for `cargo xtask dist --target windows-nsis` (xtask/src/desktop.rs); per-user, no admin.
; No game data here: with -DWITH_GAME xtask stages game\ plus game_install.nsh / game_uninstall.nsh (personal builds).
; Saves and port.cfg stay in %APPDATA%\th08, never in the install folder.
Unicode true
ManifestDPIAware true
!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"

!define APP "th08 (Touhou 8 port)"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\th08-port"

Name "${APP}"
OutFile "${OUT}"
InstallDir "$LOCALAPPDATA\Programs\th08"
InstallDirRegKey HKCU "${UNINSTALL_KEY}" "InstallLocation"
RequestExecutionLevel user
SetCompressor /SOLID lzma

!define MUI_ICON "${SRC}\th08.ico"
!define MUI_UNICON "${SRC}\th08.ico"
!define MUI_FINISHPAGE_RUN "$INSTDIR\th08.exe"
!define MUI_PAGE_CUSTOMFUNCTION_LEAVE DedicatedDir
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

VIProductVersion "${VI_VERSION}"
VIFileVersion "${VI_VERSION}"
VIAddVersionKey "ProductName" "${APP}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "FileVersion" "${VERSION}"
!ifdef WITH_GAME
VIAddVersionKey "FileDescription" "${APP} setup (personal build)"
VIAddVersionKey "LegalCopyright" "GPL-3.0-or-later (port); game data not redistributable"
!else
VIAddVersionKey "FileDescription" "${APP} setup"
VIAddVersionKey "LegalCopyright" "GPL-3.0-or-later"
!endif

; The uninstaller removes the folder, so a chosen folder that is not th08's own (D:\Games) gets th08\ appended.
Function DedicatedDir
  ${IfNot} ${FileExists} "$INSTDIR\th08.exe"
    StrCpy $R0 $INSTDIR 1 -1
    ${If} $R0 == "\"
      StrCpy $INSTDIR $INSTDIR -1
    ${EndIf}
    ${GetFileName} $INSTDIR $R0
    ${If} $R0 != "th08"
      StrCpy $INSTDIR "$INSTDIR\th08"
    ${EndIf}
  ${EndIf}
FunctionEnd

Function .onInit
  ; (A silent install skips the directory page: `/D=` gets the same rule.)
  ${If} ${Silent}
    Call DedicatedDir
  ${EndIf}
FunctionEnd

Section
  SetOutPath "$INSTDIR"
  File "${SRC}\th08.exe"
  File "${SRC}\th08.ico"
  File "${SRC}\LICENSE"
!ifdef WITH_GAME
  SetOutPath "$INSTDIR\game"
  !include "${SRC}\game_install.nsh"
  SetOutPath "$INSTDIR"
!endif
  WriteUninstaller "$INSTDIR\uninstall.exe"

  CreateShortcut "$SMPROGRAMS\${APP}.lnk" "$INSTDIR\th08.exe" "" "$INSTDIR\th08.ico"
  CreateShortcut "$DESKTOP\${APP}.lnk" "$INSTDIR\th08.exe" "" "$INSTDIR\th08.ico"

  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "${APP}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\th08.ico"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "EstimatedSize" "$0"
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\th08.exe"
  Delete "$INSTDIR\th08.ico"
  Delete "$INSTDIR\LICENSE"
!ifdef WITH_GAME
  !include "${SRC}\game_uninstall.nsh"
  RMDir "$INSTDIR\game"
!endif
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\${APP}.lnk"
  Delete "$DESKTOP\${APP}.lnk"
  DeleteRegKey HKCU "${UNINSTALL_KEY}"
SectionEnd
