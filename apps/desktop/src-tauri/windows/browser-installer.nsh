!macro NSIS_HOOK_POSTINSTALL
  nsExec::ExecToStack '"$INSTDIR\jyroxa-browser-register.exe" install "$INSTDIR\jyroxa-desktop.exe"'
  Pop $0
  Pop $1
  ${If} $0 != 0
    DetailPrint "Jyroxa browser integration registration failed ($0): $1"
    SetErrorLevel 1
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::ExecToStack '"$INSTDIR\jyroxa-browser-register.exe" uninstall'
  Pop $0
  Pop $1
!macroend
