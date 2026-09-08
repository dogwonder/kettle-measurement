//! #428's public export contract, exercised through the local workflow command.
use serde_json::{json, Value};
use std::path::Path;
use std::process::Command;

#[test]
fn field_export_contains_counts_and_shape_metadata_but_no_document_content() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let work = std::env::temp_dir().join(format!("kettle-field-{}", std::process::id()));
    std::fs::create_dir_all(work.join("run")).unwrap();
    let secret = "private source 9 May 2026 £789.12 /person/account.pdf";
    std::fs::write(
        work.join("run/results.json"),
        json!({"obligations":[{"ask":secret}]}).to_string(),
    )
    .unwrap();
    std::fs::write(work.join("provenance.private.json"), json!({"model":"test model", "pack":"test pack", "runtime":"test runtime", "basis":"synthetic command test"}).to_string()).unwrap();
    let invoke = |args: &[&str]| {
        Command::new("python3")
            .arg(root.join("scripts/field-evidence.py"))
            .args(args)
            .current_dir(&work)
            .output()
            .unwrap()
    };
    let init = invoke(&[
        "init",
        "--run-dir",
        "run",
        "--provenance",
        "provenance.private.json",
        "--format",
        "pdf",
        "--structure",
        "table",
        "--coverage",
        "amount-selection",
        "--out",
        "observation.private.json",
    ]);
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    let observation = work.join("observation.private.json");
    let mut data: Value = serde_json::from_slice(&std::fs::read(&observation).unwrap()).unwrap();
    data["adjudications"][0]["outcome"] = json!("wrong");
    data["adjudications"][0]["note"] = json!(secret);
    std::fs::write(&observation, data.to_string()).unwrap();
    let result = invoke(&["export", "--record", "observation.private.json"]);
    assert!(result.status.success());
    let output = String::from_utf8(result.stdout).unwrap();
    let value: Value = serde_json::from_str(&output).unwrap();
    let schema: Value = serde_json::from_str(include_str!(
        "../../../evals/corpus/field-summary.schema.json"
    ))
    .unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    assert!(validator.is_valid(&value));
    assert_eq!(value["outcomes"]["wrong"], 1);
    for forbidden in [
        secret,
        "£789.12",
        "9 May",
        "account.pdf",
        "test model",
        "test runtime",
    ] {
        assert!(!output.contains(forbidden));
    }
    let mut leaked = value;
    leaked["source"] = json!(secret);
    assert!(!validator.is_valid(&leaked));
    std::fs::remove_dir_all(work).unwrap();
}
