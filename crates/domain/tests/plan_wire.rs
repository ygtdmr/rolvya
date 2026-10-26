use rolvya_domain::plan::{AccessMode, Effort, WriteScope};
#[test]
fn read_only_scope_rejects_undeclared_fields() {
    for value in [
        r#"{"kind":"read_only","paths":["secrets"]}"#,
        r#"{"kind":"read_only","repository":"x"}"#,
    ] {
        assert!(
            serde_json::from_str::<WriteScope>(value).is_err(),
            "discarded undeclared write scope"
        );
    }
}
#[test]
fn unknown_assignments_do_not_fall_back() {
    assert!(serde_json::from_str::<AccessMode>(r#""unknown""#).is_err());
    assert!(serde_json::from_str::<Effort>(r#""automatic""#).is_err());
}
