!macro NSIS_HOOK_POSTINSTALL
  ; ---------------------------------------------------------------------------
  ; Detect first install vs. update.
  ; If our RegisteredApplications entry doesn't exist yet, this is a first
  ; install and we'll prompt for a restart at the end (Explorer needs a
  ; restart to fully pick up new file-handler registrations + COM classes).
  ; ---------------------------------------------------------------------------
  StrCpy $R8 "0" ; $R8 = "1" on first install only (for auto-start registration)
  StrCpy $R9 "0" ; $R9 = "1" when reboot needed (first install OR fresh WinFsp)
  ReadRegStr $R0 HKCU "Software\RegisteredApplications" "CarmineDesktop"
  ${If} $R0 == ""
    StrCpy $R8 "1"
    StrCpy $R9 "1"
  ${EndIf}

  ; WinFsp registers under WOW6432Node (32-bit view). Tauri's NSIS template
  ; sets SetRegView 64, so we must switch to 32-bit to find it.
  SetRegView 32

  ; Check if WinFsp is already installed via registry
  ReadRegStr $0 HKLM "SOFTWARE\WinFsp" "InstallDir"
  ${If} $0 != ""
    DetailPrint "WinFsp already installed at $0, skipping."
  ${Else}
    DetailPrint "Installing WinFsp..."

    ${If} ${FileExists} "$INSTDIR\resources\winfsp.msi"
      ; Copy bundled MSI to temp and run silent install
      CopyFiles "$INSTDIR\resources\winfsp.msi" "$TEMP\winfsp.msi"
      ExecWait 'msiexec /i "$TEMP\winfsp.msi" /qn INSTALLLEVEL=1000' $1

      ; Clean up temp copy
      Delete "$TEMP\winfsp.msi"

      ${If} $1 != 0
        MessageBox MB_OK|MB_ICONSTOP "WinFsp installation failed (exit code: $1).$\n$\nCarmine Desktop requires WinFsp to function. Please install WinFsp manually from https://winfsp.dev."
      ${Else}
        DetailPrint "WinFsp installed successfully."
        ; WinFsp is a kernel driver — a restart is required.
        StrCpy $R9 "1"
      ${EndIf}
    ${EndIf}
  ${EndIf}

  ; Clean up bundled MSI from install directory (not needed at runtime)
  Delete "$INSTDIR\resources\winfsp.msi"

  ; Restore 64-bit view for Tauri's remaining NSIS steps
  SetRegView 64

  ; ---------------------------------------------------------------------------
  ; Register Carmine Desktop in the Windows file association system.
  ;
  ; This uses the modern Windows 10/11 model: RegisteredApplications +
  ; Capabilities + OpenWithProgids.  The runtime code in setup_after_launch()
  ; refreshes these on every launch, but writing them at install time ensures
  ; Explorer knows about Carmine Desktop immediately after a restart.
  ; ---------------------------------------------------------------------------

  DetailPrint "Registering file associations..."

  ; Application capabilities
  WriteRegStr HKCU "Software\CarmineDesktop\Capabilities" "ApplicationDescription" "Mounts SharePoint and OneDrive as local drives"
  WriteRegStr HKCU "Software\CarmineDesktop\Capabilities" "ApplicationName" "Carmine Desktop"
  WriteRegStr HKCU "Software\CarmineDesktop\Capabilities\FileAssociations" ".docx" "CarmineDesktop.OfficeFile.docx"
  WriteRegStr HKCU "Software\CarmineDesktop\Capabilities\FileAssociations" ".xlsx" "CarmineDesktop.OfficeFile.xlsx"
  WriteRegStr HKCU "Software\CarmineDesktop\Capabilities\FileAssociations" ".pptx" "CarmineDesktop.OfficeFile.pptx"
  WriteRegStr HKCU "Software\CarmineDesktop\Capabilities\FileAssociations" ".doc" "CarmineDesktop.OfficeFile.doc"
  WriteRegStr HKCU "Software\CarmineDesktop\Capabilities\FileAssociations" ".xls" "CarmineDesktop.OfficeFile.xls"
  WriteRegStr HKCU "Software\CarmineDesktop\Capabilities\FileAssociations" ".ppt" "CarmineDesktop.OfficeFile.ppt"
  WriteRegStr HKCU "Software\RegisteredApplications" "CarmineDesktop" "Software\CarmineDesktop\Capabilities"

  ; ProgID entries with shell\open\command
  WriteRegStr HKCU "Software\Classes\CarmineDesktop.OfficeFile.docx" "" "Word Document (Carmine Desktop)"
  WriteRegStr HKCU "Software\Classes\CarmineDesktop.OfficeFile.docx\shell\open\command" "" '"$INSTDIR\Carmine Desktop.exe" --open "%1"'
  WriteRegStr HKCU "Software\Classes\CarmineDesktop.OfficeFile.xlsx" "" "Excel Spreadsheet (Carmine Desktop)"
  WriteRegStr HKCU "Software\Classes\CarmineDesktop.OfficeFile.xlsx\shell\open\command" "" '"$INSTDIR\Carmine Desktop.exe" --open "%1"'
  WriteRegStr HKCU "Software\Classes\CarmineDesktop.OfficeFile.pptx" "" "PowerPoint Presentation (Carmine Desktop)"
  WriteRegStr HKCU "Software\Classes\CarmineDesktop.OfficeFile.pptx\shell\open\command" "" '"$INSTDIR\Carmine Desktop.exe" --open "%1"'
  WriteRegStr HKCU "Software\Classes\CarmineDesktop.OfficeFile.doc" "" "Word Document (Carmine Desktop)"
  WriteRegStr HKCU "Software\Classes\CarmineDesktop.OfficeFile.doc\shell\open\command" "" '"$INSTDIR\Carmine Desktop.exe" --open "%1"'
  WriteRegStr HKCU "Software\Classes\CarmineDesktop.OfficeFile.xls" "" "Excel Spreadsheet (Carmine Desktop)"
  WriteRegStr HKCU "Software\Classes\CarmineDesktop.OfficeFile.xls\shell\open\command" "" '"$INSTDIR\Carmine Desktop.exe" --open "%1"'
  WriteRegStr HKCU "Software\Classes\CarmineDesktop.OfficeFile.ppt" "" "PowerPoint Presentation (Carmine Desktop)"
  WriteRegStr HKCU "Software\Classes\CarmineDesktop.OfficeFile.ppt\shell\open\command" "" '"$INSTDIR\Carmine Desktop.exe" --open "%1"'

  ; OpenWithProgids entries (makes us appear in "Open with" dialog)
  WriteRegStr HKCU "Software\Classes\.docx\OpenWithProgids" "CarmineDesktop.OfficeFile.docx" ""
  WriteRegStr HKCU "Software\Classes\.xlsx\OpenWithProgids" "CarmineDesktop.OfficeFile.xlsx" ""
  WriteRegStr HKCU "Software\Classes\.pptx\OpenWithProgids" "CarmineDesktop.OfficeFile.pptx" ""
  WriteRegStr HKCU "Software\Classes\.doc\OpenWithProgids" "CarmineDesktop.OfficeFile.doc" ""
  WriteRegStr HKCU "Software\Classes\.xls\OpenWithProgids" "CarmineDesktop.OfficeFile.xls" ""
  WriteRegStr HKCU "Software\Classes\.ppt\OpenWithProgids" "CarmineDesktop.OfficeFile.ppt" ""

  ; Notify Explorer that file associations changed
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0x0000, p 0, p 0)'

  DetailPrint "File associations registered."

  ; ---------------------------------------------------------------------------
  ; On first install, register auto-start (enabled by default).
  ;
  ; The runtime reconciliation in setup_after_launch() keeps the registry in
  ; sync with the user's preference on every subsequent launch, but writing
  ; the Run key at install time ensures the app starts after the first-install
  ; reboot — before the app has ever had a chance to run.
  ; Only on first install so updates don't override a user who disabled it.
  ; ---------------------------------------------------------------------------
  ${If} $R8 == "1"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" \
      "Carmine Desktop" '$INSTDIR\Carmine Desktop.exe'
    DetailPrint "Auto-start registered."
  ${EndIf}

  ; ---------------------------------------------------------------------------
  ; On first install, prompt for a restart.
  ;
  ; Explorer caches COM class registrations and file handler lists.
  ; SHChangeNotify refreshes association data but the "Open with" list and
  ; the IApplicationAssociationRegistrationUI COM class require an Explorer
  ; restart (logoff/logon) or a full system restart to become visible.
  ; We only prompt on first install — updates don't need it.
  ; ---------------------------------------------------------------------------
  ${If} $R9 == "1"
    MessageBox MB_YESNO|MB_ICONQUESTION \
      "A system restart is recommended so Windows fully recognizes Carmine Desktop for opening Office files.$\n$\nRestart now?" \
      /SD IDNO IDNO skip_reboot
      Reboot
    skip_reboot:
  ${EndIf}
!macroend

; ---------------------------------------------------------------------------
; Helper: restore a single extension's previous handler during uninstall.
;
; Must be defined at top level (NSIS forbids macros inside macros).
; Read CarmineDesktop.PreviousHandler BEFORE deleting it, and write the value
; back as the extension's default ProgID.  If no previous handler was saved,
; clear the default so Windows can fall back naturally.
;
; Also attempt to delete UserChoice keys — these contain stale ProgId +
; invalid hash after our ProgIDs are removed.  Deletion may fail due to ACL
; protection; that's OK — Explorer will fall back gracefully.
; ---------------------------------------------------------------------------
!macro _RestoreExtension EXT PROGID
  ; Read saved previous handler (into $R0)
  ReadRegStr $R0 HKCU "Software\Classes\${EXT}" "CarmineDesktop.PreviousHandler"
  ${If} $R0 != ""
    ; Restore previous handler as extension default
    WriteRegStr HKCU "Software\Classes\${EXT}" "" "$R0"
  ${Else}
    ; No saved handler — clear the default (currently points to our ProgID)
    DeleteRegValue HKCU "Software\Classes\${EXT}" ""
  ${EndIf}

  ; Delete saved previous handler value
  DeleteRegValue HKCU "Software\Classes\${EXT}" "CarmineDesktop.PreviousHandler"

  ; Remove from OpenWithProgids (delete value, not the key itself)
  DeleteRegValue HKCU "Software\Classes\${EXT}\OpenWithProgids" "${PROGID}"

  ; Attempt to delete UserChoice key (may fail due to ACL — non-fatal)
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\${EXT}\UserChoice"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; ---------------------------------------------------------------------------
  ; Clean up file association registry keys written by POSTINSTALL and runtime.
  ;
  ; Order matters: we must restore previous handlers BEFORE deleting ProgIDs,
  ; otherwise the extension defaults are left pointing to deleted keys.
  ; ---------------------------------------------------------------------------

  ; Remove auto-start registry entry
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Carmine Desktop"

  ; Remove RegisteredApplications entry
  DeleteRegValue HKCU "Software\RegisteredApplications" "CarmineDesktop"

  ; Remove Capabilities tree
  DeleteRegKey HKCU "Software\CarmineDesktop\Capabilities"
  DeleteRegKey HKCU "Software\CarmineDesktop"

  ; Restore previous handlers for each extension
  !insertmacro _RestoreExtension ".docx" "CarmineDesktop.OfficeFile.docx"
  !insertmacro _RestoreExtension ".xlsx" "CarmineDesktop.OfficeFile.xlsx"
  !insertmacro _RestoreExtension ".pptx" "CarmineDesktop.OfficeFile.pptx"
  !insertmacro _RestoreExtension ".doc"  "CarmineDesktop.OfficeFile.doc"
  !insertmacro _RestoreExtension ".xls"  "CarmineDesktop.OfficeFile.xls"
  !insertmacro _RestoreExtension ".ppt"  "CarmineDesktop.OfficeFile.ppt"

  ; Remove ProgID entries (AFTER restoring defaults, so defaults don't point to deleted keys)
  DeleteRegKey HKCU "Software\Classes\CarmineDesktop.OfficeFile.docx"
  DeleteRegKey HKCU "Software\Classes\CarmineDesktop.OfficeFile.xlsx"
  DeleteRegKey HKCU "Software\Classes\CarmineDesktop.OfficeFile.pptx"
  DeleteRegKey HKCU "Software\Classes\CarmineDesktop.OfficeFile.doc"
  DeleteRegKey HKCU "Software\Classes\CarmineDesktop.OfficeFile.xls"
  DeleteRegKey HKCU "Software\Classes\CarmineDesktop.OfficeFile.ppt"

  ; Notify Explorer
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0x0000, p 0, p 0)'
!macroend
