//! Contract tests for the results cache (#18). The key is
//! (pack id, pack version, BLAKE3 input hash, model id) — a hit means
//! re-running is a no-op that hands back the prior run directory.
//!
//! Authored red as the handoff contract: make these green by
//! implementing `runner::cache`, don't edit them. See the CONTRACT FILE
//! note in `src/cache.rs`.

use runner::cache::{cache_key, Cache};
use std::path::{Path, PathBuf};

/// A per-test scratch directory. Tests share one process, so pid + name
/// is unique enough (same posture as the pack-loader tests).
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kettle-cache-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

fn write(dir: &Path, name: &str, content: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, content).expect("write scratch input");
    path
}

const STATEMENT: &str = "Date,Description,Amount\n2025-01-08,NETFLIX.COM,-10.99\n";

#[test]
fn same_inputs_same_key() {
    let dir = scratch("same-inputs");
    let input = write(&dir, "statement.csv", STATEMENT);

    let first = cache_key(
        "app.kttl.subscription-audit",
        "1.0.0",
        std::slice::from_ref(&input),
        "qwen-7b",
    )
    .expect("hash inputs");
    let second = cache_key("app.kttl.subscription-audit", "1.0.0", &[input], "qwen-7b")
        .expect("hash inputs");

    assert_eq!(first, second);
}

#[test]
fn any_component_changes_key() {
    let dir = scratch("components");
    let input = write(&dir, "statement.csv", STATEMENT);
    let inputs = [input.clone()];

    let base =
        cache_key("app.kttl.subscription-audit", "1.0.0", &inputs, "qwen-7b").expect("hash inputs");

    let other_pack = cache_key("letter-to-actions", "1.0.0", &inputs, "qwen-7b").expect("hash");
    let other_version =
        cache_key("app.kttl.subscription-audit", "1.0.1", &inputs, "qwen-7b").expect("hash");
    let other_model =
        cache_key("app.kttl.subscription-audit", "1.0.0", &inputs, "qwen-14b").expect("hash");

    // Same path, one row more: the content hash must move.
    write(
        &dir,
        "statement.csv",
        &format!("{STATEMENT}2025-02-08,SPOTIFY,-11.99\n"),
    );
    let other_content =
        cache_key("app.kttl.subscription-audit", "1.0.0", &inputs, "qwen-7b").expect("hash inputs");

    for (what, key) in [
        ("pack id", &other_pack),
        ("pack version", &other_version),
        ("model id", &other_model),
        ("input content", &other_content),
    ] {
        assert_ne!(&base, key, "{what} must change the cache key");
    }
}

/// The pipeline concatenates statements before grouping, so the order
/// files were picked in cannot change the answer — and must not miss
/// the cache.
#[test]
fn input_order_does_not_change_key() {
    let dir = scratch("order");
    let january = write(&dir, "january.csv", STATEMENT);
    let february = write(
        &dir,
        "february.csv",
        "Date,Description,Amount\n2025-02-08,SPOTIFY,-11.99\n",
    );

    let forwards = cache_key(
        "app.kttl.subscription-audit",
        "1.0.0",
        &[january.clone(), february.clone()],
        "qwen-7b",
    )
    .expect("hash inputs");
    let backwards = cache_key(
        "app.kttl.subscription-audit",
        "1.0.0",
        &[february, january],
        "qwen-7b",
    )
    .expect("hash inputs");

    assert_eq!(forwards, backwards);
}

/// Content, not location: the same statement copied elsewhere produces
/// the same run, so it should hit the same cache entry.
#[test]
fn same_content_different_path_same_key() {
    let dir = scratch("content");
    let here = write(&dir, "statement.csv", STATEMENT);
    let there = write(&dir, "copy-of-statement.csv", STATEMENT);

    let first =
        cache_key("app.kttl.subscription-audit", "1.0.0", &[here], "qwen-7b").expect("hash");
    let second =
        cache_key("app.kttl.subscription-audit", "1.0.0", &[there], "qwen-7b").expect("hash");

    assert_eq!(first, second);
}

#[test]
fn unreadable_input_is_an_error() {
    let dir = scratch("unreadable");
    let missing = dir.join("not-here.csv");

    assert!(cache_key(
        "app.kttl.subscription-audit",
        "1.0.0",
        &[missing],
        "qwen-7b"
    )
    .is_err());
}

#[test]
fn lookup_misses_until_recorded() {
    let dir = scratch("miss-then-hit");
    let input = write(&dir, "statement.csv", STATEMENT);
    let key = cache_key("app.kttl.subscription-audit", "1.0.0", &[input], "qwen-7b").expect("hash");

    let cache = Cache::new(dir.join("cache"));
    assert_eq!(cache.lookup(&key), None);

    let run_dir = dir.join("runs/run-01");
    std::fs::create_dir_all(&run_dir).expect("create run dir");
    cache.record(&key, &run_dir).expect("record run");

    assert_eq!(cache.lookup(&key), Some(run_dir));
}

/// "Delete everything" must actually delete everything: a cache entry
/// pointing at a run directory that's gone is a miss, not a crash and
/// not a phantom hit.
#[test]
fn lookup_misses_when_the_run_directory_is_gone() {
    let dir = scratch("deleted-run");
    let input = write(&dir, "statement.csv", STATEMENT);
    let key = cache_key("app.kttl.subscription-audit", "1.0.0", &[input], "qwen-7b").expect("hash");

    let cache = Cache::new(dir.join("cache"));
    let run_dir = dir.join("runs/run-01");
    std::fs::create_dir_all(&run_dir).expect("create run dir");
    cache.record(&key, &run_dir).expect("record run");
    std::fs::remove_dir_all(&run_dir).expect("delete run dir");

    assert_eq!(cache.lookup(&key), None);
}

/// Two runs, two entries: recording one must not evict or shadow the
/// other.
#[test]
fn entries_do_not_shadow_each_other() {
    let dir = scratch("two-entries");
    let input = write(&dir, "statement.csv", STATEMENT);
    let small = cache_key(
        "app.kttl.subscription-audit",
        "1.0.0",
        std::slice::from_ref(&input),
        "qwen-7b",
    )
    .expect("hash");
    let large =
        cache_key("app.kttl.subscription-audit", "1.0.0", &[input], "qwen-14b").expect("hash");

    let cache = Cache::new(dir.join("cache"));
    for (key, name) in [(&small, "run-01"), (&large, "run-02")] {
        let run_dir = dir.join("runs").join(name);
        std::fs::create_dir_all(&run_dir).expect("create run dir");
        cache.record(key, &run_dir).expect("record run");
    }

    assert_eq!(cache.lookup(&small), Some(dir.join("runs/run-01")));
    assert_eq!(cache.lookup(&large), Some(dir.join("runs/run-02")));
}

// ---------------------------------------------------------------------------
// Deletion and containment (#109, #113). A cache entry is a pointer to
// somebody's private run. Deleting the run has to take the pointer with
// it, and a pointer that leads anywhere but the runs root is not
// something to hand back and act on.

/// #109: deleting one run must not leave an entry that would hand that
/// run back to the next identical ask. The entry is the last thing
/// naming it, so it goes too.
#[test]
fn forgetting_a_run_removes_every_entry_pointing_at_it() {
    let dir = scratch("forget-one");
    let input = write(&dir, "statement.csv", STATEMENT);
    let seven = cache_key(
        "app.kttl.subscription-audit",
        "1.0.0",
        std::slice::from_ref(&input),
        "qwen-7b",
    )
    .expect("hash");
    // A second key for the same run: two models can't produce one run
    // in practice, but the cache must not assume one entry per run.
    let also_seven = cache_key(
        "app.kttl.subscription-audit",
        "1.0.1",
        std::slice::from_ref(&input),
        "qwen-7b",
    )
    .expect("hash");
    let fourteen =
        cache_key("app.kttl.subscription-audit", "1.0.0", &[input], "qwen-14b").expect("hash");

    let cache = Cache::new(dir.join("cache"));
    let doomed = dir.join("runs/run-01");
    let kept = dir.join("runs/run-02");
    for run_dir in [&doomed, &kept] {
        std::fs::create_dir_all(run_dir).expect("create run dir");
    }
    cache.record(&seven, &doomed).expect("record");
    cache.record(&also_seven, &doomed).expect("record");
    cache.record(&fourteen, &kept).expect("record");

    assert_eq!(
        cache.forget(&doomed).expect("forget"),
        2,
        "both entries naming that run are gone"
    );
    std::fs::remove_dir_all(&doomed).expect("delete the run itself");

    assert_eq!(cache.lookup(&seven), None);
    assert_eq!(cache.lookup(&also_seven), None);
    assert_eq!(
        cache.lookup(&fourteen),
        Some(kept),
        "another run's entry is untouched"
    );
}

/// Forgetting a run nothing points at is a no-op, not an error — the
/// caller is deleting a run, and whether it was ever cached is not
/// their problem.
#[test]
fn forgetting_an_uncached_run_is_not_an_error() {
    let dir = scratch("forget-uncached");
    let cache = Cache::new(dir.join("cache"));
    assert_eq!(cache.forget(&dir.join("runs/run-99")).expect("forget"), 0);
}

/// #109: "delete everything" includes the index of what was kept.
#[test]
fn clearing_forgets_every_entry() {
    let dir = scratch("clear-all");
    let input = write(&dir, "statement.csv", STATEMENT);
    let key = cache_key("app.kttl.subscription-audit", "1.0.0", &[input], "qwen-7b").expect("hash");
    let run_dir = dir.join("runs/run-01");
    std::fs::create_dir_all(&run_dir).expect("create run dir");
    let cache = Cache::new(dir.join("cache"));
    cache.record(&key, &run_dir).expect("record");

    cache.clear().expect("clear");

    assert_eq!(cache.lookup(&key), None);
    // The run itself is not the cache's to delete — only the pointer.
    assert!(
        run_dir.is_dir(),
        "clearing the index kept no opinion about runs"
    );
}

/// #113: an entry is a path read off the disk. One leading anywhere but
/// the runs root — a stale absolute path, a hand-edited file, a `..` —
/// must read as a miss rather than as a directory to serve a report
/// from. The cache never broadens what Kettle will touch.
#[test]
fn an_entry_pointing_outside_the_runs_root_is_a_miss() {
    let dir = scratch("escaping-entry");
    let input = write(&dir, "statement.csv", STATEMENT);
    let key = cache_key("app.kttl.subscription-audit", "1.0.0", &[input], "qwen-7b").expect("hash");
    let runs = dir.join("runs");
    let elsewhere = dir.join("somewhere-else");
    std::fs::create_dir_all(&runs).expect("runs root");
    std::fs::create_dir_all(&elsewhere).expect("elsewhere");

    let cache = Cache::new(dir.join("cache"));
    cache.record(&key, &elsewhere).expect("record");

    assert_eq!(
        cache.lookup_within(&runs, &key),
        None,
        "a run directory outside the runs root is not a hit"
    );
    // …and the same entry inside the root is.
    let inside = runs.join("run-01");
    std::fs::create_dir_all(&inside).expect("run dir");
    cache.record(&key, &inside).expect("record");
    assert_eq!(cache.lookup_within(&runs, &key), Some(inside));
}

/// The obvious escape, spelled the obvious way. `..` is resolved before
/// the question is asked, not pattern-matched.
#[test]
fn a_dot_dot_entry_does_not_climb_out_of_the_runs_root() {
    let dir = scratch("dot-dot-entry");
    let input = write(&dir, "statement.csv", STATEMENT);
    let key = cache_key("app.kttl.subscription-audit", "1.0.0", &[input], "qwen-7b").expect("hash");
    let runs = dir.join("runs");
    let outside = dir.join("private");
    std::fs::create_dir_all(runs.join("run-01")).expect("runs root");
    std::fs::create_dir_all(&outside).expect("outside");

    let cache = Cache::new(dir.join("cache"));
    cache
        .record(&key, &runs.join("../private"))
        .expect("record");

    assert_eq!(cache.lookup_within(&runs, &key), None);
}
