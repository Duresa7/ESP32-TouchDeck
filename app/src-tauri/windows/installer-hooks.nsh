!macro NSIS_HOOK_POSTUNINSTALL
  ; The template deletes the main binary once. Right after an update another process can still hold
  ; it for a moment, which left touchdeck.exe behind; retry for 10 seconds, then defer to reboot.
  StrCmp $UpdateMode "1" touchdeck_main_done
  Push $R0
  StrCpy $R0 0
touchdeck_main_retry:
  IfFileExists "$INSTDIR\${MAINBINARYNAME}.exe" 0 touchdeck_main_removed
  IntCmp $R0 40 touchdeck_main_deferred
  Sleep 250
  IntOp $R0 $R0 + 1
  Delete "$INSTDIR\${MAINBINARYNAME}.exe"
  Goto touchdeck_main_retry
touchdeck_main_deferred:
  Delete /REBOOTOK "$INSTDIR\${MAINBINARYNAME}.exe"
touchdeck_main_removed:
  RMDir "$INSTDIR"
  Pop $R0
touchdeck_main_done:
  ClearErrors
!macroend
