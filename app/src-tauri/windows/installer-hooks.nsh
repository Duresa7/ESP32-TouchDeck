!macro NSIS_HOOK_PREINSTALL
  ; Remove bundles installed by older releases before copying the fixed resource name.
  Delete "$INSTDIR\firmware\*.tdfw"

  ; A status refresh may still be executing the old helper after TouchDeck exits.
  ; Move its image on the same volume so File can write the replacement immediately.
  Push $R0
  Delete "$INSTDIR\td-statusline.exe"
  IfFileExists "$INSTDIR\td-statusline.exe" 0 touchdeck_helper_ready
  GetTempFileName $R0 "$INSTDIR"
  Delete "$R0"
  ClearErrors
  Rename "$INSTDIR\td-statusline.exe" "$R0"
  IfErrors touchdeck_helper_failed
  Delete /REBOOTOK "$R0"
  Goto touchdeck_helper_ready
touchdeck_helper_failed:
  Pop $R0
  Abort "Could not replace td-statusline.exe. Close status line sessions and try again."
touchdeck_helper_ready:
  Pop $R0
  ClearErrors
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  StrCmp $UpdateMode "1" touchdeck_unwrap_done
  IfFileExists "$INSTDIR\td-statusline.exe" 0 touchdeck_unwrap_done
  Push $R0
  nsExec::ExecToLog '"$INSTDIR\td-statusline.exe" --unwrap'
  Pop $R0
  Pop $R0
touchdeck_unwrap_done:
  ClearErrors
!macroend

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
