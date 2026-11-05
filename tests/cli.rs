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
fn reimbursement_route_catalog_is_available_offline() {
    let d = tempfile::tempdir().unwrap();
    let s = d.path().join("cookies.json");
    let o = cli()
        .arg("--session")
        .arg(&s)
        .arg("--compact")
        .args(["thu", "wsbx.routes"])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    let routes = v["routes"].as_array().unwrap();
    assert!(routes.iter().any(|route| route["ywlx"] == "1206"));
    assert!(routes
        .iter()
        .any(|route| route["save_path"] == "/clf/gn/tmp/save"));
}

#[test]
fn conference_preview_builds_payload_without_auth() {
    let d = tempfile::tempdir().unwrap();
    let s = d.path().join("cookies.json");
    let args = r#"{"ywbh":"YB_FIXTURE_19","ywlx":"19","project":{"bmbh":"999","xmbh":"FIXTURE"},"phone":"10000000000","conference":{"hyfBaseInfo":{"hymc":"Fixture"},"hyfZcmxList":[{"jjflkmbh":"302"}]}}"#;
    let o = cli()
        .arg("--session")
        .arg(&s)
        .arg("--compact")
        .args(["thu", "wsbx.draft.preview", "--args", args])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["network_request_sent"], false);
    assert_eq!(v["save_path"], "/pay/whole/submit");
    assert_eq!(v["payload"]["hyfBaseInfo"]["ywbh"], "YB_FIXTURE_19");
}

#[test]
fn global_help_and_version() {
    assert!(cli().arg("--help").status().unwrap().success());
    assert!(cli().arg("--version").status().unwrap().success());
}
