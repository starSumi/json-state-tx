use json_state_tx::{Options, apply, init, read, version_bytes};
use proptest::prelude::*;
use serde_json::json;
use std::fs;
use tempfile::tempdir;

#[test]
fn init_read_and_apply_are_versioned() {
    let dir = tempdir().unwrap();
    let state = dir.path().join("state.json");
    let options = Options::default();
    let initial = br#"{"counter":0,"name":"demo"}"#;

    let created = init(&state, initial, None, &options).unwrap();
    assert_eq!(created.previous_version, None);
    assert_eq!(read(&state, &options).unwrap().version, created.version);

    let patch = br#"[{"op":"replace","path":"/counter","value":1}]"#;
    let committed = apply(&state, patch, &created.version, None, &options).unwrap();
    assert_eq!(committed.document["counter"], 1);
    assert_eq!(
        committed.previous_version.as_deref(),
        Some(created.version.as_str())
    );
    assert_eq!(version_bytes(&fs::read(&state).unwrap()), committed.version);
}

#[test]
fn stale_writer_does_not_change_state() {
    let dir = tempdir().unwrap();
    let state = dir.path().join("state.json");
    let options = Options::default();
    let created = init(&state, br#"{"counter":0}"#, None, &options).unwrap();
    let patch = br#"[{"op":"replace","path":"/counter","value":1}]"#;
    let first = apply(&state, patch, &created.version, None, &options).unwrap();

    let error = apply(&state, patch, &created.version, None, &options).unwrap_err();
    assert_eq!(error.code, "VERSION_CONFLICT");
    assert_eq!(read(&state, &options).unwrap().version, first.version);
}

#[test]
fn failing_patch_and_schema_leave_old_bytes() {
    let dir = tempdir().unwrap();
    let state = dir.path().join("state.json");
    let options = Options::default();
    let schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": {"counter": {"type": "integer", "minimum": 0}},
        "required": ["counter"],
        "additionalProperties": false
    });
    let created = init(&state, br#"{"counter":0}"#, Some(&schema), &options).unwrap();
    let before = fs::read(&state).unwrap();

    let test_fail = br#"[{"op":"test","path":"/counter","value":99},{"op":"replace","path":"/counter","value":1}]"#;
    assert_eq!(
        apply(&state, test_fail, &created.version, Some(&schema), &options)
            .unwrap_err()
            .code,
        "PATCH_APPLY_FAILED"
    );
    assert_eq!(fs::read(&state).unwrap(), before);

    let schema_fail = br#"[{"op":"replace","path":"/counter","value":-1}]"#;
    assert_eq!(
        apply(
            &state,
            schema_fail,
            &created.version,
            Some(&schema),
            &options
        )
        .unwrap_err()
        .code,
        "SCHEMA_REJECTED"
    );
    assert_eq!(fs::read(&state).unwrap(), before);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]
    #[test]
    fn replacement_round_trips_and_old_version_becomes_stale(initial in any::<i64>(), next in any::<i64>()) {
        let dir = tempdir().unwrap();
        let state = dir.path().join("state.json");
        let options = Options::default();
        let initial_bytes = serde_json::to_vec(&json!({"value": initial})).unwrap();
        let created = init(&state, &initial_bytes, None, &options).unwrap();
        let patch = serde_json::to_vec(&json!([{"op": "replace", "path": "/value", "value": next}])).unwrap();
        let committed = apply(&state, &patch, &created.version, None, &options).unwrap();
        prop_assert_eq!(committed.document["value"].as_i64(), Some(next));
        prop_assert_eq!(apply(&state, &patch, &created.version, None, &options).unwrap_err().code, "VERSION_CONFLICT");
    }
}
