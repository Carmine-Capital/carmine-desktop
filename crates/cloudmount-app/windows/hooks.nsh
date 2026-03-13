!macro NSIS_HOOK_PREINSTALL
  ; Check if WinFsp is already installed via registry
  ReadRegStr $0 HKLM "SOFTWARE\WinFsp" "InstallDir"
  ${If} $0 != ""
    DetailPrint "WinFsp already installed at $0, skipping."
  ${Else}
    DetailPrint "Installing WinFsp..."

    ; Extract the bundled WinFsp MSI to temp directory
    SetOutPath "$TEMP"
    File "${PROJECT_DIR}\resources\winfsp.msi"

    ; Run silent install with full feature set
    ExecWait 'msiexec /i "$TEMP\winfsp.msi" /qn INSTALLLEVEL=1000' $1

    ; Clean up temp MSI regardless of outcome
    Delete "$TEMP\winfsp.msi"

    ; Check exit code
    ${If} $1 != 0
      MessageBox MB_OK|MB_ICONSTOP "WinFsp installation failed (exit code: $1).$\n$\nCloudMount requires WinFsp to function. Please install WinFsp manually from https://winfsp.dev and retry."
      Abort
    ${EndIf}

    DetailPrint "WinFsp installed successfully."
  ${EndIf}
!macroend
