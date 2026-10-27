#![cfg(target_os = "linux")]
use rolvya_persistence_sqlite::secret_broker::{MAX_SECRET_BYTES, SecretValue};
#[test]
fn protected_values_are_bounded_and_redacted() {
    let value = SecretValue::new(b"synthetic-confidential-value".to_vec()).unwrap();
    assert!(!format!("{value:?}").contains("synthetic-confidential-value"));
    assert!(SecretValue::new(vec![0; MAX_SECRET_BYTES + 1]).is_err());
}
