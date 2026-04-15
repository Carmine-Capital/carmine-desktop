!macro NSIS_HOOK_POSTINSTALL
  ; ---------------------------------------------------------------------------
  ; Detect first install vs. update.
  ; If our RegisteredApplications entry doesn't exist yet, this is a first
  ; install and we'll write the Run key so auto-start works before the user
  ; launches the app for the first time.
  ; ---------------------------------------------------------------------------
  StrCpy $R8 "0" ; $R8 = "1" on first install only (for auto-start registration)
  ReadRegStr $R0 HKCU "Software\RegisteredApplications" "CarmineDesktop"
  ${If} $R0 == ""
    StrCpy $R8 "1"
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
      ${EndIf}
    ${EndIf}
  ${EndIf}

  ; Clean up bundled MSI from install directory (not needed at runtime)
  Delete "$INSTDIR\resources\winfsp.msi"

  ; Restore 64-bit view for Tauri's remaining NSIS steps
  SetRegView 64

  ; ---------------------------------------------------------------------------
  ; On first install, register auto-start (enabled by default).
  ;
  ; The runtime reconciliation in setup_after_launch() keeps the registry in
  ; sync with the user's preference on every subsequent launch, but writing
  ; the Run key at install time ensures the app starts after a reboot — before
  ; the app has ever had a chance to run.
  ; Only on first install so updates don't override a user who disabled it.
  ; ---------------------------------------------------------------------------
  ${If} $R8 == "1"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" \
      "Carmine Desktop" '$INSTDIR\Carmine Desktop.exe'
    DetailPrint "Auto-start registered."
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; ---------------------------------------------------------------------------
  ; Clean up registry keys written by the runtime (and the auto-start key
  ; written by POSTINSTALL).
  ;
  ; The runtime registers Carmine in the modern Windows file-association
  ; system (Capabilities + RegisteredApplications + ProgID tree under
  ; HKCU\Software\Classes\CarmineDesktop.OfficeFile.*) but never overwrites
  ; the per-extension default handler. So uninstall just deletes our own
  ; keys — there is no previous handler to restore.
  ; ---------------------------------------------------------------------------

  ; Remove auto-start registry entry
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Carmine Desktop"

  ; Remove RegisteredApplications entry
  DeleteRegValue HKCU "Software\RegisteredApplications" "CarmineDesktop"

  ; Remove Capabilities tree
  DeleteRegKey HKCU "Software\CarmineDesktop\Capabilities"
  DeleteRegKey HKCU "Software\CarmineDesktop"

  ; Remove our ProgID entries (HKCU\Software\Classes\CarmineDesktop.OfficeFile.*)
  DeleteRegKey HKCU "Software\Classes\CarmineDesktop.OfficeFile.docx"
  DeleteRegKey HKCU "Software\Classes\CarmineDesktop.OfficeFile.xlsx"
  DeleteRegKey HKCU "Software\Classes\CarmineDesktop.OfficeFile.pptx"
  DeleteRegKey HKCU "Software\Classes\CarmineDesktop.OfficeFile.doc"
  DeleteRegKey HKCU "Software\Classes\CarmineDesktop.OfficeFile.xls"
  DeleteRegKey HKCU "Software\Classes\CarmineDesktop.OfficeFile.ppt"

  ; Drop our entry from each extension's OpenWithProgids list (delete value,
  ; not the key itself — other apps may share it).
  DeleteRegValue HKCU "Software\Classes\.docx\OpenWithProgids" "CarmineDesktop.OfficeFile.docx"
  DeleteRegValue HKCU "Software\Classes\.xlsx\OpenWithProgids" "CarmineDesktop.OfficeFile.xlsx"
  DeleteRegValue HKCU "Software\Classes\.pptx\OpenWithProgids" "CarmineDesktop.OfficeFile.pptx"
  DeleteRegValue HKCU "Software\Classes\.doc\OpenWithProgids"  "CarmineDesktop.OfficeFile.doc"
  DeleteRegValue HKCU "Software\Classes\.xls\OpenWithProgids"  "CarmineDesktop.OfficeFile.xls"
  DeleteRegValue HKCU "Software\Classes\.ppt\OpenWithProgids"  "CarmineDesktop.OfficeFile.ppt"

  ; Notify Explorer that file associations changed
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0x0000, p 0, p 0)'
!macroend
