//! #282: a run that was interrupted can pick up where it stopped.
//!
//! The value is obvious — a large letter bed (#242) lost three times in
//! one afternoon. The danger is not: a cache that reuses a
//! result it should not silently reports numbers nobody measured, and
//! that is worse than losing the run. So most of what is asserted here
//! is what must *miss*.

use runner::eval::resume::{ResumeCache, ResumeKey};
use runner::eval::{EvalSet, FixtureResult, Perf, StepScore};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn cache_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kettle-resume-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// A key with every field populated, so each test can spoil exactly one.
fn key() -> ResumeKey {
    ResumeKey {
        pack: "app.kttl.subscription-audit".to_owned(),
        pack_version: "1.3.0".to_owned(),
        prompt_version: "blake3:abc123".to_owned(),
        model: "qwen2.5-7b-instruct-q4_k_m.gguf".to_owned(),
        sidecar: "10050 (b15ca938a)".to_owned(),
        scoring_version: runner::eval::SCORING_VERSION,
        eval_set: EvalSet::Development,
        fixture: "statement-02-messy.csv".to_owned(),
        fixture_digest: "blake3:deadbeef".to_owned(),
    }
}

fn result(score: f32) -> FixtureResult {
    let mut step_scores = BTreeMap::new();
    step_scores.insert(
        "normalise".to_owned(),
        StepScore {
            score,
            expected: 10,
            correct: (score * 10.0) as usize,
        },
    );
    FixtureResult {
        fixture: "statement-02-messy.csv".to_owned(),
        step_scores,
        items: Vec::new(),
        containment: Default::default(),
        end_to_end: score,
        needs_review_rate: 0.25,
        retries: 0,
        perf: Some(Perf::default()),
        stability: None,
    }
}

#[test]
fn a_cached_fixture_is_reused_whole_not_only_its_items() {
    // The finding the spike turned up: the per-fixture file written
    // today holds only `items`, so a resume built on it would lose
    // end_to_end and the review rate — the two numbers a verdict is
    // actually read from. The cache unit is the whole result.
    let dir = cache_dir("whole");
    let cache = ResumeCache::at(&dir);
    cache.put(&key(), &result(0.75)).expect("cached");

    let found = cache.get(&key()).expect("a hit");
    assert_eq!(found.end_to_end, 0.75, "the end result survives");
    assert_eq!(found.needs_review_rate, 0.25, "and so does the review rate");
    assert_eq!(found.step_scores["normalise"].score, 0.75);
    assert_eq!(found.step_scores["normalise"].expected, 10);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_legacy_cached_retry_count_is_not_lost_on_resume() {
    let dir = cache_dir("legacy-retries");
    let cache = ResumeCache::at(&dir);
    cache.put(&key(), &result(0.75)).expect("cached");

    let path = std::fs::read_dir(&dir)
        .expect("cache dir")
        .next()
        .expect("cache entry")
        .expect("entry")
        .path();
    let mut document: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read cache entry"))
            .expect("cache JSON");
    document
        .as_object_mut()
        .expect("fixture object")
        .remove("retries");
    document["perf"]["retries"] = serde_json::json!(2);
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&document).expect("cache serialises"),
    )
    .expect("write legacy cache entry");

    let found = cache.get(&key()).expect("legacy cache still hits");
    assert_eq!(found.retries, 2, "resume retains the recorded retries");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_regenerated_fixture_is_a_miss_even_though_its_name_did_not_change() {
    // The failure that would otherwise be invisible, and the reason
    // the digest is in the key at all. `kettle bed` rewrites every
    // fixture in place: same names, different letters, different
    // expectations. Keyed on the name alone, a resumed eval would
    // score the old statements against the new expectations and
    // nobody would be told.
    let dir = cache_dir("regenerated");
    let cache = ResumeCache::at(&dir);
    cache.put(&key(), &result(0.75)).expect("cached");

    let regenerated = ResumeKey {
        fixture_digest: "blake3:0ddba11".to_owned(),
        ..key()
    };
    assert!(
        cache.get(&regenerated).is_none(),
        "a fixture whose bytes changed must be measured again"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_field_of_the_key_independently_forces_a_miss() {
    let dir = cache_dir("fields");
    let cache = ResumeCache::at(&dir);
    cache.put(&key(), &result(0.75)).expect("cached");

    // Each spoiled field, and the wrong answer reusing it would give.
    let spoiled: Vec<(&str, ResumeKey)> = vec![
        (
            "a 1.2.0 fixture scored into a 1.3.0 run",
            ResumeKey {
                pack_version: "1.2.0".to_owned(),
                ..key()
            },
        ),
        (
            "a prompt edit half-measured — the one change this project cannot review",
            ResumeKey {
                prompt_version: "blake3:different".to_owned(),
                ..key()
            },
        ),
        (
            "one model's answers credited to another",
            ResumeKey {
                model: "qwen3.5-9b-q4_k_m.gguf".to_owned(),
                ..key()
            },
        ),
        (
            "a llama-server bump changes grammar-constrained sampling on its own (#74)",
            ResumeKey {
                sidecar: "10100 (deadbeef)".to_owned(),
                ..key()
            },
        ),
        (
            "numbers that no longer mean the same thing",
            ResumeKey {
                scoring_version: runner::eval::SCORING_VERSION + 1,
                ..key()
            },
        ),
        (
            "the sealed exam set spent by accident",
            ResumeKey {
                eval_set: EvalSet::Exam,
                ..key()
            },
        ),
        (
            "one statement's score read as another's",
            ResumeKey {
                fixture: "statement-01.csv".to_owned(),
                ..key()
            },
        ),
    ];

    for (harm, spoiled) in spoiled {
        assert!(
            cache.get(&spoiled).is_none(),
            "this key must miss, or the result is: {harm}"
        );
    }

    // And the untouched key still hits, so the test above is not
    // passing merely because nothing ever hits.
    assert!(cache.get(&key()).is_some(), "the unspoiled key still hits");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_absent_cache_is_a_miss_and_never_an_error() {
    // A resume asked for on a machine with no cache is a full run, not
    // a failure. Losing a measurement because a directory was missing
    // would be the wrong way round, the same posture as run logging.
    let dir = cache_dir("absent");
    let cache = ResumeCache::at(&dir);
    assert!(cache.get(&key()).is_none());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_corrupt_cache_entry_is_a_miss_and_never_an_error() {
    // Half a file, from the interruption that made resume necessary in
    // the first place. Re-measuring is always safe; trusting a
    // truncated record is not.
    let dir = cache_dir("corrupt");
    let cache = ResumeCache::at(&dir);
    cache.put(&key(), &result(0.75)).expect("cached");

    for entry in std::fs::read_dir(&dir).expect("cache dir") {
        let path = entry.expect("entry").path();
        std::fs::write(&path, "{\"fixture\": \"statement-02-mes").expect("truncate");
    }

    assert!(
        cache.get(&key()).is_none(),
        "a half-written record must be re-measured, not believed"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_resumed_run_scores_the_same_and_says_it_was_resumed() {
    // End to end on the floor, which needs no model: measure once,
    // measure again against a warm cache, and the report must be the
    // same measurement — plus an honest count of what it reused.
    use runner::eval::fixture::FixtureEvaluator;
    use runner::eval::MachineInfo;
    use runner::packs::load_pack;
    use runner::run::Answers;

    let dir = cache_dir("end-to-end");
    let pack_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packs/app.kttl.subscription-audit");
    let pack = load_pack(&pack_dir).expect("pack loads");
    let evaluator = || FixtureEvaluator {
        answers: Answers::WithoutModel,
        model: None,
        machine: MachineInfo {
            cpu: "Apple M1 Pro".to_owned(),
            ram_gb: 32,
            os: "macOS 15.5".to_owned(),
        },
        sidecar: None,
        peak_rss: None,
        fixtures_dir: None,
        runs_dir: None,
        resume_dir: Some(dir.clone()),
    };

    let cold = evaluator().evaluate(&pack).expect("the first run");
    assert_eq!(cold.reused_fixtures, 0, "nothing to reuse on a cold cache");

    let warm = evaluator().evaluate(&pack).expect("the resumed run");
    assert_eq!(
        warm.reused_fixtures,
        cold.fixtures.len(),
        "every fixture came from the cache"
    );

    // The point of the whole exercise: resuming must not change the
    // measurement. Scores, per-fixture results and the verdict are the
    // same run, assembled across two sittings.
    assert_eq!(warm.verdict, cold.verdict);
    assert_eq!(warm.fixtures.len(), cold.fixtures.len());
    for (warm_fixture, cold_fixture) in warm.fixtures.iter().zip(&cold.fixtures) {
        assert_eq!(warm_fixture.fixture, cold_fixture.fixture);
        assert_eq!(warm_fixture.end_to_end, cold_fixture.end_to_end);
        assert_eq!(
            warm_fixture.needs_review_rate,
            cold_fixture.needs_review_rate
        );
        assert_eq!(warm_fixture.step_scores, cold_fixture.step_scores);
        assert_eq!(warm_fixture.items.len(), cold_fixture.items.len());
    }
    assert_eq!(warm.metrics, cold.metrics, "the aggregates are the same");

    let _ = std::fs::remove_dir_all(&dir);
}
