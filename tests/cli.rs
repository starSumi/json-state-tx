use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::process::{Command as ProcessCommand, Stdio};
use tempfile::tempdir;

#[test]
fn cli_emits_machine_readable_envelopes_and_conflict_code() {
    let dir = tempdir().unwrap();
    let state = dir.path().join("state.json");
    let document = dir.path().join("document.json");
    let patch = dir.path().join("patch.json");
    fs::write(&document, br#"{"value":1}"#).unwrap();
    fs::write(&patch, br#"[{"op":"replace","path":"/value","value":2}]"#).unwrap();

    let init = Command::cargo_bin("json-state-tx")
        .unwrap()
        .args([
            "init",
            "--file",
            state.to_str().unwrap(),
            "--document",
            document.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(init.status.success());
    let envelope: serde_json::Value = serde_json::from_slice(&init.stdout).unwrap();
    let version = envelope["result"]["version"].as_str().unwrap();

    Command::cargo_bin("json-state-tx")
        .unwrap()
        .args([
            "apply",
            "--file",
            state.to_str().unwrap(),
            "--patch",
            patch.to_str().unwrap(),
            "--if-version",
            version,
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"ok\":true"));

    Command::cargo_bin("json-state-tx")
        .unwrap()
        .args([
            "apply",
            "--file",
            state.to_str().unwrap(),
            "--patch",
            patch.to_str().unwrap(),
            "--if-version",
            version,
        ])
        .assert()
        .code(20)
        .stdout(predicate::str::contains("VERSION_CONFLICT"));
}

#[test]
fn concurrent_writers_have_exactly_one_winner() {
    let dir = tempdir().unwrap();
    let state = dir.path().join("state.json");
    let document = dir.path().join("document.json");
    let patch = dir.path().join("patch.json");
    fs::write(&document, br#"{"winner":null}"#).unwrap();
    fs::write(
        &patch,
        br#"[{"op":"replace","path":"/winner","value":"one"}]"#,
    )
    .unwrap();

    let binary = assert_cmd::cargo::cargo_bin!("json-state-tx");
    let init = ProcessCommand::new(binary)
        .args([
            "init",
            "--file",
            state.to_str().unwrap(),
            "--document",
            document.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let envelope: serde_json::Value = serde_json::from_slice(&init.stdout).unwrap();
    let version = envelope["result"]["version"].as_str().unwrap().to_owned();

    let children: Vec<_> = (0..16)
        .map(|_| {
            ProcessCommand::new(binary)
                .args([
                    "apply",
                    "--file",
                    state.to_str().unwrap(),
                    "--patch",
                    patch.to_str().unwrap(),
                    "--if-version",
                    &version,
                ])
                .stdout(Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();

    let codes: Vec<_> = children
        .into_iter()
        .map(|child| child.wait_with_output().unwrap().status.code().unwrap())
        .collect();
    assert_eq!(codes.iter().filter(|code| **code == 0).count(), 1);
    assert_eq!(codes.iter().filter(|code| **code == 20).count(), 15);
}
