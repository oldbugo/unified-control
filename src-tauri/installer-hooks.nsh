; Unified Control installer hooks (Tauri NSIS, per-user install, no admin). Copied from CoolerScreen's installer-hooks.nsh;
; the CoolerScreen 1.x clean-up and shortcut repointing are not here (Unified Control has no older install to replace).
; Unified Control installs into its own folder (%LOCALAPPDATA%\Unified Control) with its own Apps & Features entry, Start Menu
; shortcut and data folder (%APPDATA%\UnifiedControl). Nothing here touches CoolerScreen's files, data or start-up entry.

!macro NSIS_HOOK_PREINSTALL
!macroend

!macro NSIS_HOOK_POSTINSTALL
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; The opt-in "Start Unified Control when Windows starts" entry must not outlive the app.
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "UnifiedControl"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
!macroend
