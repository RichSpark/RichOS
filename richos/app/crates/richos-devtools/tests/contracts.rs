use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};

fn rpc(operation: &str, input: &Value) -> (i32, Value) {
    let mut process = Command::new(env!("CARGO_BIN_EXE_richos-devtools"))
        .args(["rpc", operation])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    process
        .stdin
        .take()
        .unwrap()
        .write_all(input.to_string().as_bytes())
        .unwrap();
    let output = process.wait_with_output().unwrap();
    (
        output.status.code().unwrap(),
        serde_json::from_slice(&output.stdout).unwrap(),
    )
}

#[test]
fn original_outputs_are_preserved() {
    // Captured from the Python originals at 47ed2e43973c. Synthetic inputs only.
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/original-outputs.json")).unwrap();
    assert_eq!(cases.len(), 59);
    for (index, case) in cases.iter().enumerate() {
        let (code, output) = rpc(case["operation"].as_str().unwrap(), &case["input"]);
        assert_eq!(code, 0, "case {index}: {output}");
        assert_eq!(output["result"], case["expected"], "case {index}");
    }
}

#[test]
fn bad_requests_refuse_instead_of_succeeding_empty() {
    for (operation, value) in [
        ("cargo-summary", json!("compiler error")),
        ("unknown", Value::Null),
    ] {
        let (code, output) = rpc(operation, &value);
        assert_eq!(code, 2);
        assert!(output["error"].as_str().is_some_and(|s| !s.is_empty()));
        assert!(output.get("result").is_none());
    }
}
