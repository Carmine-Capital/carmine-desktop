use cloudmount_core::open_online::office_uri;

const TEST_URL: &str = "https://contoso.sharepoint.com/sites/eng/Shared%20Documents/report.docx";

// --- Word extensions ---

#[test]
fn test_open_online_office_uri_docx() {
    let result = office_uri(".docx", TEST_URL);
    if cfg!(target_os = "linux") {
        assert_eq!(result, TEST_URL);
    } else {
        assert_eq!(result, format!("ms-word:ofe|u|{TEST_URL}"));
    }
}

#[test]
fn test_open_online_office_uri_doc() {
    let result = office_uri(".doc", TEST_URL);
    if cfg!(target_os = "linux") {
        assert_eq!(result, TEST_URL);
    } else {
        assert_eq!(result, format!("ms-word:ofe|u|{TEST_URL}"));
    }
}

#[test]
fn test_open_online_office_uri_docm() {
    let result = office_uri(".docm", TEST_URL);
    if cfg!(target_os = "linux") {
        assert_eq!(result, TEST_URL);
    } else {
        assert_eq!(result, format!("ms-word:ofe|u|{TEST_URL}"));
    }
}

// --- Excel extensions ---

#[test]
fn test_open_online_office_uri_xlsx() {
    let result = office_uri(".xlsx", TEST_URL);
    if cfg!(target_os = "linux") {
        assert_eq!(result, TEST_URL);
    } else {
        assert_eq!(result, format!("ms-excel:ofe|u|{TEST_URL}"));
    }
}

#[test]
fn test_open_online_office_uri_xls() {
    let result = office_uri(".xls", TEST_URL);
    if cfg!(target_os = "linux") {
        assert_eq!(result, TEST_URL);
    } else {
        assert_eq!(result, format!("ms-excel:ofe|u|{TEST_URL}"));
    }
}

#[test]
fn test_open_online_office_uri_xlsm() {
    let result = office_uri(".xlsm", TEST_URL);
    if cfg!(target_os = "linux") {
        assert_eq!(result, TEST_URL);
    } else {
        assert_eq!(result, format!("ms-excel:ofe|u|{TEST_URL}"));
    }
}

// --- PowerPoint extensions ---

#[test]
fn test_open_online_office_uri_pptx() {
    let result = office_uri(".pptx", TEST_URL);
    if cfg!(target_os = "linux") {
        assert_eq!(result, TEST_URL);
    } else {
        assert_eq!(result, format!("ms-powerpoint:ofe|u|{TEST_URL}"));
    }
}

#[test]
fn test_open_online_office_uri_ppt() {
    let result = office_uri(".ppt", TEST_URL);
    if cfg!(target_os = "linux") {
        assert_eq!(result, TEST_URL);
    } else {
        assert_eq!(result, format!("ms-powerpoint:ofe|u|{TEST_URL}"));
    }
}

#[test]
fn test_open_online_office_uri_pptm() {
    let result = office_uri(".pptm", TEST_URL);
    if cfg!(target_os = "linux") {
        assert_eq!(result, TEST_URL);
    } else {
        assert_eq!(result, format!("ms-powerpoint:ofe|u|{TEST_URL}"));
    }
}

// --- Non-Office / unknown extensions ---

#[test]
fn test_open_online_office_uri_pdf_returns_plain_url() {
    let url = "https://contoso.sharepoint.com/sites/eng/Shared%20Documents/report.pdf";
    assert_eq!(office_uri(".pdf", url), url);
}

#[test]
fn test_open_online_office_uri_txt_returns_plain_url() {
    let url = "https://contoso.sharepoint.com/sites/eng/Shared%20Documents/notes.txt";
    assert_eq!(office_uri(".txt", url), url);
}

#[test]
fn test_open_online_office_uri_png_returns_plain_url() {
    let url = "https://contoso.sharepoint.com/sites/eng/Shared%20Documents/image.png";
    assert_eq!(office_uri(".png", url), url);
}

#[test]
fn test_open_online_office_uri_empty_extension_returns_plain_url() {
    assert_eq!(office_uri("", TEST_URL), TEST_URL);
}

// --- Case insensitivity ---

#[test]
fn test_open_online_office_uri_case_insensitive() {
    let result = office_uri(".DOCX", TEST_URL);
    if cfg!(target_os = "linux") {
        assert_eq!(result, TEST_URL);
    } else {
        assert_eq!(result, format!("ms-word:ofe|u|{TEST_URL}"));
    }
}

#[test]
fn test_open_online_office_uri_mixed_case() {
    let result = office_uri(".Xlsx", TEST_URL);
    if cfg!(target_os = "linux") {
        assert_eq!(result, TEST_URL);
    } else {
        assert_eq!(result, format!("ms-excel:ofe|u|{TEST_URL}"));
    }
}
