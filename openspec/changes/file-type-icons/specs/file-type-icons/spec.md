## ADDED Requirements

### Requirement: Per-file-type icon resources embedded in Windows executable

The Windows executable SHALL contain distinct icon resources for each supported file type group: Word documents (doc/docx), Excel spreadsheets (xls/xlsx), PowerPoint presentations (ppt/pptx), and PDF documents. Each icon resource SHALL be assigned a stable ordinal (101=doc, 102=xls, 103=ppt, 104=pdf) via a `.rc` resource script compiled at build time.

#### Scenario: Icon resources present in built executable
- **WHEN** the Windows executable is built with the `desktop` feature
- **THEN** the executable SHALL contain icon resources at ordinals 101, 102, 103, and 104, each with 16x16, 32x32, 48x48, and 256x256 pixel sizes

#### Scenario: Build on non-Windows platforms
- **WHEN** the project is built on Linux or macOS
- **THEN** the resource compilation step SHALL be skipped without error

### Requirement: File association registration sets per-type DefaultIcon

When registering file associations on Windows, the system SHALL create a `DefaultIcon` subkey under each ProgID key, referencing the corresponding icon resource ordinal from the executable using the negative ordinal syntax (`,-N`).

#### Scenario: Registering file associations sets correct icons
- **WHEN** `register_file_associations()` is called on Windows
- **THEN** each ProgID key (`CarmineDesktop.OfficeFile.{ext}`) SHALL have a `DefaultIcon` subkey with value `"{exe_path},-{ordinal}"` where the ordinal matches the extension's file type group (101 for .doc/.docx, 102 for .xls/.xlsx, 103 for .ppt/.pptx)

#### Scenario: Extension-to-ordinal mapping
- **WHEN** a `.docx` or `.doc` file is associated
- **THEN** the DefaultIcon SHALL reference ordinal 101
- **WHEN** a `.xlsx` or `.xls` file is associated
- **THEN** the DefaultIcon SHALL reference ordinal 102
- **WHEN** a `.pptx` or `.ppt` file is associated
- **THEN** the DefaultIcon SHALL reference ordinal 103

### Requirement: File association unregistration cleans up DefaultIcon

When unregistering file associations, the `DefaultIcon` subkey SHALL be removed along with the rest of the ProgID key tree.

#### Scenario: Unregistering removes icon references
- **WHEN** `unregister_file_associations()` is called on Windows
- **THEN** the entire ProgID key tree (including `DefaultIcon`) SHALL be deleted, as already handled by `delete_subkey_all`

### Requirement: ICO files committed as build artifacts

Multi-resolution `.ico` files (16, 32, 48, 256px) SHALL exist at `crates/carminedesktop-app/icons/files/` for each file type (doc.ico, xls.ico, ppt.ico, pdf.ico), generated offline from the corresponding SVG source files.

#### Scenario: ICO files present in repository
- **WHEN** a developer clones the repository
- **THEN** `icons/files/doc.ico`, `icons/files/xls.ico`, `icons/files/ppt.ico`, and `icons/files/pdf.ico` SHALL be present and valid Windows ICO files
