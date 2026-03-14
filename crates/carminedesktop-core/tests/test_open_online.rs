use carminedesktop_core::open_online::is_collaborative;

#[test]
fn test_open_online_is_collaborative_office_documents() {
    assert!(is_collaborative(".docx"));
    assert!(is_collaborative(".xlsx"));
    assert!(is_collaborative(".pptx"));
}

#[test]
fn test_open_online_is_collaborative_legacy_office() {
    assert!(is_collaborative(".doc"));
    assert!(is_collaborative(".xls"));
    assert!(is_collaborative(".ppt"));
}

#[test]
fn test_open_online_is_collaborative_macro_formats() {
    assert!(is_collaborative(".docm"));
    assert!(is_collaborative(".xlsm"));
    assert!(is_collaborative(".pptm"));
}

#[test]
fn test_open_online_is_collaborative_odf() {
    assert!(is_collaborative(".odt"));
    assert!(is_collaborative(".ods"));
    assert!(is_collaborative(".odp"));
}

#[test]
fn test_open_online_is_collaborative_visio() {
    assert!(is_collaborative(".vsdx"));
}

#[test]
fn test_open_online_is_collaborative_non_collaborative() {
    assert!(!is_collaborative(".pdf"));
    assert!(!is_collaborative(".txt"));
    assert!(!is_collaborative(".csv"));
    assert!(!is_collaborative(".jpg"));
    assert!(!is_collaborative(".png"));
}

#[test]
fn test_open_online_is_collaborative_case_insensitive() {
    assert!(is_collaborative(".DOCX"));
    assert!(is_collaborative(".Xlsx"));
    assert!(is_collaborative(".PPTX"));
}

#[test]
fn test_open_online_is_collaborative_unknown() {
    assert!(!is_collaborative(".xyz"));
    assert!(!is_collaborative(""));
    assert!(!is_collaborative("docx"));
}
