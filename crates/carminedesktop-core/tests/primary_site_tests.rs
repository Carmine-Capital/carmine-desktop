use carminedesktop_core::primary_site;

#[test]
fn test_primary_site_constants_have_expected_values() {
    assert_eq!(
        primary_site::SITE_ID,
        "carminecapital.sharepoint.com,a6465300-0266-4e82-8862-42b51dc22851,f866b89d-f622-4ed2-a776-51f8521f136c"
    );
    assert_eq!(primary_site::SITE_NAME, "Carmine Capital");
}
