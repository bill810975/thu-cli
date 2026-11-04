use serde_json::Value;
use std::process::Command;
fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_thu-cli"))
}
#[test]
fn catalog_is_unique_and_explicit() {
    let o = cli().arg("services").output().unwrap();
    assert!(o.status.success());
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    for service in ["thu", "learn", "app"] {
        let ops = v[service].as_array().unwrap();
        assert!(!ops.is_empty());
        let mut seen = std::collections::HashSet::new();
        for op in ops {
            assert!(seen.insert(op["name"].as_str().unwrap()));
            assert!(op["mutating"].is_boolean());
            assert!(op["required"].is_array());
            assert!(op["optional"].is_array());
        }
    }
}
#[test]
fn dry_run_does_not_create_session_or_make_network_request() {
    let d = tempfile::tempdir().unwrap();
    let s = d.path().join("cookies.json");
    let o = cli()
        .arg("--session")
        .arg(&s)
        .args([
            "learn",
            "homework-submit",
            "--args",
            "{\"id\":\"fake-offline-id\"}",
            "--dry-run",
        ])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(!s.exists());
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["network_request_sent"], false);
}
#[test]
fn mutation_without_confirmation_fails_before_session() {
    let d = tempfile::tempdir().unwrap();
    let s = d.path().join("cookies.json");
    let o = cli()
        .arg("--session")
        .arg(&s)
        .args(["learn", "homework-submit", "--args", "{\"id\":\"fake\"}"])
        .output()
        .unwrap();
    assert!(!o.status.success());
    assert!(!s.exists());
    assert!(String::from_utf8_lossy(&o.stderr).contains("--confirm"));
}
#[test]
fn missing_required_argument_fails_offline() {
    let o = cli()
        .args(["learn", "courses", "--dry-run"])
        .output()
        .unwrap();
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("semester"));
}

#[test]
fn reimbursement_submit_requires_confirmation_before_network() {
    let d = tempfile::tempdir().unwrap();
    let s = d.path().join("cookies.json");
    let args = r#"{"ywbh":"YB_FIXTURE","project":{"bmbh":"999","xmbh":"FIXTURE"},"phone":"10000000000","items":[]}"#;
    let o = cli()
        .arg("--session")
        .arg(&s)
        .args(["thu", "wsbx.draft.submit", "--args", args])
        .output()
        .unwrap();
    assert!(!o.status.success());
    assert!(!s.exists());
    assert!(String::from_utf8_lossy(&o.stderr).contains("--confirm"));
}

#[test]
fn reimbursement_submit_dry_run_is_offline() {
    let d = tempfile::tempdir().unwrap();
    let s = d.path().join("cookies.json");
    let args = r#"{"ywbh":"YB_FIXTURE","project":{"bmbh":"999","xmbh":"FIXTURE"},"phone":"10000000000","items":[]}"#;
    let o = cli()
        .arg("--session")
        .arg(&s)
        .args(["thu", "wsbx.draft.submit", "--args", args, "--dry-run"])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(!s.exists());
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["network_request_sent"], false);
}

#[test]
fn global_help_and_version() {
    assert!(cli().arg("--help").status().unwrap().success());
    assert!(cli().arg("--version").status().unwrap().success());
}
