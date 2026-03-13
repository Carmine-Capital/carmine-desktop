/// Returns `true` if the file extension is editable collaboratively via Microsoft 365 Online.
pub fn is_collaborative(extension: &str) -> bool {
    matches!(
        extension.to_ascii_lowercase().as_str(),
        ".doc"
            | ".docx"
            | ".docm"
            | ".xls"
            | ".xlsx"
            | ".xlsm"
            | ".ppt"
            | ".pptx"
            | ".pptm"
            | ".odt"
            | ".ods"
            | ".odp"
            | ".vsdx"
    )
}
