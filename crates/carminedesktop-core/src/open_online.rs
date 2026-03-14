/// Build an Office URI scheme string (`ms-word:ofe|u|<url>`) for the given extension
/// and **direct** document URL (not a `_layouts/15/Doc.aspx` web view URL).
///
/// Returns `None` on Linux or for non-Office file types.
pub fn office_uri(extension: &str, direct_url: &str) -> Option<String> {
    let scheme = office_uri_scheme(extension)?;
    Some(format!("{scheme}:ofe|u|{direct_url}"))
}

/// Returns the Office URI scheme for the given file extension.
fn office_uri_scheme(extension: &str) -> Option<&'static str> {
    if cfg!(target_os = "linux") {
        return None;
    }

    match extension.to_ascii_lowercase().as_str() {
        ".doc" | ".docx" | ".docm" => Some("ms-word"),
        ".xls" | ".xlsx" | ".xlsm" => Some("ms-excel"),
        ".ppt" | ".pptx" | ".pptm" => Some("ms-powerpoint"),
        _ => None,
    }
}

/// Build a direct SharePoint document URL from the drive's web URL and item metadata.
///
/// The drive `web_url` is the document library root (e.g.
/// `https://tenant-my.sharepoint.com/personal/user/Documents`).
/// `parent_path` is the item's `parentReference.path` (e.g. `/drives/{id}/root:/folder`).
/// The relative portion after `root:` is appended, then the file `name`.
pub fn direct_document_url(drive_web_url: &str, parent_path: &str, name: &str) -> String {
    let relative = parent_path
        .split_once("root:")
        .map(|(_, rel)| rel.trim_start_matches('/'))
        .unwrap_or("");

    let base = drive_web_url.trim_end_matches('/');
    if relative.is_empty() {
        format!("{base}/{name}")
    } else {
        format!("{base}/{relative}/{name}")
    }
}

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
