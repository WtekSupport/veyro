Function VeyroStopRunningApp
  nsExec::ExecToLog 'taskkill /F /IM veyro.exe /T'
  Pop $0
FunctionEnd

Function un.VeyroStopRunningApp
  nsExec::ExecToLog 'taskkill /F /IM veyro.exe /T'
  Pop $0
FunctionEnd

!macro NSIS_HOOK_PREINSTALL
  Call VeyroStopRunningApp
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  Call un.VeyroStopRunningApp
!macroend
