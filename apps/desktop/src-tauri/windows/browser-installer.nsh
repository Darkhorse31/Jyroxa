!macro NSIS_HOOK_POSTINSTALL
  ExecWait '"$INSTDIR\jyroxa-browser-register.exe" install "$INSTDIR\jyroxa-desktop.exe"' $0
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ExecWait '"$INSTDIR\jyroxa-browser-register.exe" uninstall' $0
!macroend