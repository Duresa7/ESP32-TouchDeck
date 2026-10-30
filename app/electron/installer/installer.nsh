; Open TouchDeck additions to the electron-builder NSIS template.
!include LogicLib.nsh
!include WordFunc.nsh
; The template skips these when customCheckAppRunning is defined; the stock check below needs them.
!include getProcessInfo.nsh
Var pid

!define TOUCHDECK_DATA "io.github.duresa7.touchdeck"
!define TOUCHDECK_RUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"
!define TOUCHDECK_RUN_VALUE "Open TouchDeck"

; The stock check force-stops every process in $INSTDIR, which would kill the Companion mid-write.
; Ask the Companion to shut down first, then let the stock check close the window.
!macro customCheckAppRunning
  InitPluginsDir
  File "/oname=$PLUGINSDIR\stop-companion.ps1" "${BUILD_RESOURCES_DIR}\stop-companion.ps1"
  DetailPrint "Stopping the Open TouchDeck Companion"
  nsExec::ExecToLog `"$PowerShellPath" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PLUGINSDIR\stop-companion.ps1" -InstallDir "$INSTDIR"`
  Pop $0
  ${if} $0 == 3
    MessageBox MB_OK|MB_ICONEXCLAMATION "Open TouchDeck is updating a Device. Try again when it finishes." /SD IDOK
    SetErrorLevel 3
    Quit
  ${elseif} $0 != 0
    nsExec::Exec `"$CmdPath" /C taskkill /F /IM td-companion.exe /FI "USERNAME eq %USERNAME%"`
    Pop $0
  ${endif}
  !insertmacro IS_POWERSHELL_AVAILABLE
  !insertmacro _CHECK_APP_RUNNING
!macroend

!macro customInit
  ReadRegStr $0 SHELL_CONTEXT "${UNINSTALL_REGISTRY_KEY}" DisplayVersion
  ${if} $0 != ""
    ${VersionCompare} "$0" "${VERSION}" $1
    ${if} $1 == 1
      MessageBox MB_OK|MB_ICONSTOP "Open TouchDeck $0 is already installed, which is newer than ${VERSION}. Uninstall it first to install this version." /SD IDOK
      SetErrorLevel 5
      Quit
    ${endif}
  ${endif}
!macroend

!macro customUnInstall
  ${ifNot} ${isUpdated}
    ReadRegStr $0 HKCU "${TOUCHDECK_RUN_KEY}" "${TOUCHDECK_RUN_VALUE}"
    StrCpy $1 `"$INSTDIR\td-companion.exe"`
    StrLen $2 $1
    StrCpy $0 $0 $2
    ${if} $0 == $1
      DeleteRegValue HKCU "${TOUCHDECK_RUN_KEY}" "${TOUCHDECK_RUN_VALUE}"
    ${endif}
    ; Without the marker, a later install registers startup at sign-in again.
    Delete "$LOCALAPPDATA\${TOUCHDECK_DATA}\startup-initialized"

    ClearErrors
    ${GetParameters} $0
    ${GetOptions} $0 "--delete-app-data" $1
    ${if} ${Errors}
      MessageBox MB_YESNO|MB_ICONQUESTION|MB_DEFBUTTON2 "Also delete your Open TouchDeck Layouts, settings and Integration settings from this PC?" /SD IDNO IDNO touchdeck_keep_data
    ${endif}
    RMDir /r "$LOCALAPPDATA\${TOUCHDECK_DATA}"
    RMDir /r "$APPDATA\${TOUCHDECK_DATA}"
    RMDir /r "$APPDATA\${PRODUCT_NAME}"
    touchdeck_keep_data:
  ${endif}
!macroend
