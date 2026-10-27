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

; NSIS_HOOK_PREUNINSTALL: reserved for td-statusline --unwrap support.
; Restore the wrapped status line before removing its helper once that mode is available.
