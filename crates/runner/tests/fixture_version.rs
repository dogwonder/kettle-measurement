use runner::eval::{fixture::Expected, SCORING_VERSION};

#[test]
fn explicitly_versioned_truth_requires_the_matching_scorer() {
    for version in [SCORING_VERSION, SCORING_VERSION + 1, SCORING_VERSION - 1] {
        let expected: Expected = serde_json::from_value(serde_json::json!({
            "scoring_version": version,
            "obligations": []
        }))
        .unwrap();
        if version == SCORING_VERSION {
            expected.validate().unwrap();
        } else {
            assert!(expected.validate().unwrap_err().contains("scoring version"));
        }
    }
}

#[test]
fn legacy_truth_uses_the_existing_validation_rules() {
    let expected: Expected = serde_json::from_str("{}").unwrap();
    expected.validate().unwrap();
}
