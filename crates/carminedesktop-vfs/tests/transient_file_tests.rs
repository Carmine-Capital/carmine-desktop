use carminedesktop_vfs::core_ops::is_transient_file;

// ── Positive cases: these should all be detected as transient ────────────

#[test]
fn test_is_transient_file_office_lock_file() {
    assert!(is_transient_file("~$Book1.xlsx"));
}

#[test]
fn test_is_transient_file_office_lock_docx() {
    assert!(is_transient_file("~$Report.docx"));
}

#[test]
fn test_is_transient_file_office_temp_file() {
    assert!(is_transient_file("~WRS0001.tmp"));
}

#[test]
fn test_is_transient_file_office_temp_uppercase() {
    assert!(is_transient_file("~DF1234.TMP"));
}

#[test]
fn test_is_transient_file_thumbs_db() {
    assert!(is_transient_file("Thumbs.db"));
}

#[test]
fn test_is_transient_file_thumbs_db_uppercase() {
    assert!(is_transient_file("THUMBS.DB"));
}

#[test]
fn test_is_transient_file_desktop_ini() {
    assert!(is_transient_file("desktop.ini"));
}

#[test]
fn test_is_transient_file_desktop_ini_capitalized() {
    assert!(is_transient_file("Desktop.ini"));
}

#[test]
fn test_is_transient_file_ds_store() {
    assert!(is_transient_file(".DS_Store"));
}

// ── Negative cases: these should NOT be detected as transient ────────────

#[test]
fn test_is_transient_file_normal_xlsx() {
    assert!(!is_transient_file("Budget Report.xlsx"));
}

#[test]
fn test_is_transient_file_tilde_no_dollar() {
    // Starts with ~ but no $ and doesn't end with .tmp
    assert!(!is_transient_file("~notes.txt"));
}

#[test]
fn test_is_transient_file_tmp_no_tilde() {
    // Ends with .tmp but doesn't start with ~
    assert!(!is_transient_file("file.tmp"));
}

#[test]
fn test_is_transient_file_thumbs_db_with_suffix() {
    // Contains thumbs.db but has extra suffix
    assert!(!is_transient_file("thumbs.db.bak"));
}

#[test]
fn test_is_transient_file_empty_string() {
    assert!(!is_transient_file(""));
}

#[test]
fn test_is_transient_file_normal_docx() {
    assert!(!is_transient_file("My Document.docx"));
}
