use std::{
    io::Write,
    os::unix::fs::OpenOptionsExt,
    process::{Command, Stdio},
};
fn executable() -> &'static str {
    env!("CARGO_BIN_EXE_rolvya-controller")
}
#[test]
fn unsafe_controller_startup_is_nonzero_and_diagnostics_do_not_echo_inputs() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("config.json");
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&path)
        .unwrap();
    file.write_all(b"{\"not_a_configuration\":\"recognizable-private-invalid-input\"}")
        .unwrap();
    for command in ["serve", "initialize", "recover-key", "unknown-command"] {
        let output = Command::new(executable())
            .arg(command)
            .arg(&path)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(!output.status.success());
        let text = String::from_utf8_lossy(&output.stderr);
        assert!(!text.contains("recognizable-private-invalid-input"));
        assert!(!text.contains(path.to_str().unwrap()));
        assert!(output.stdout.is_empty());
    }
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
}
#[test]
fn compiled_controller_has_no_public_signup_reset_or_execution_activation_command() {
    for command in [
        "signup",
        "reset-password",
        "enable-billing",
        "launch-worker",
        "--production",
    ] {
        assert!(
            !Command::new(executable())
                .arg(command)
                .stdin(Stdio::null())
                .output()
                .unwrap()
                .status
                .success()
        );
    }
}
