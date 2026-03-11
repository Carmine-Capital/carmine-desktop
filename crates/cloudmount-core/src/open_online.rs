/// Build a URI that opens the given SharePoint file in the appropriate application.
///
/// On Windows and macOS, Office file extensions are mapped to their Office URI schemes
/// (`ms-word:ofe|u|...`, `ms-excel:ofe|u|...`, `ms-powerpoint:ofe|u|...`) so the desktop
/// Office app opens with a direct SharePoint connection (co-authoring enabled).
///
/// On Linux (where desktop Office is unavailable) or for non-Office file types, the plain
/// `webUrl` is returned so it opens in the default browser.
pub fn office_uri(extension: &str, web_url: &str) -> String {
    let scheme = office_uri_scheme(extension);

    match scheme {
        Some(s) => format!("{s}:ofe|u|{web_url}"),
        None => web_url.to_string(),
    }
}

/// Returns the Office URI scheme for the given file extension, or `None` if the
/// extension is not a recognized Office type or the platform is Linux.
fn office_uri_scheme(extension: &str) -> Option<&'static str> {
    // On Linux, always fall back to the browser — desktop Office is not available.
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
