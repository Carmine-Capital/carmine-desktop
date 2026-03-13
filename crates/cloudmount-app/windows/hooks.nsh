!macro NSIS_HOOK_POSTINSTALL
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
        MessageBox MB_OK|MB_ICONSTOP "WinFsp installation failed (exit code: $1).$\n$\nCloudMount requires WinFsp to function. Please install WinFsp manually from https://winfsp.dev."
      ${Else}
        DetailPrint "WinFsp installed successfully."
      ${EndIf}
    ${EndIf}
  ${EndIf}

  ; Clean up bundled MSI from install directory (not needed at runtime)
  Delete "$INSTDIR\resources\winfsp.msi"

  ; Restore 64-bit view for Tauri's remaining NSIS steps
  SetRegView 64
!macroend
