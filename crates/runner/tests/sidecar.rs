use runner::sidecar::{Sidecar, SidecarRuntime};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn mock_binary() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mock_bin/llama-server-mock.sh")
}

/// Per-test scratch log path; tests share one process, so pid + name is
/// unique enough.
fn scratch_log(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "kettle-sidecar-test-{}-{name}.log",
        std::process::id()
    ))
}

#[test]
fn captures_child_output_to_log() {
    let log = scratch_log("captures");
    let _ = std::fs::remove_file(&log);

    let mut sidecar = Sidecar::spawn(
        &mock_binary(),
        &fake_model(),
        &log,
        SidecarRuntime::default(),
    )
    .expect("spawn mock sidecar");
    sidecar
        .wait_until_ready(Duration::from_secs(10))
        .expect("sidecar became ready");

    let logged = std::fs::read_to_string(&log).expect("read sidecar log");
    assert!(
        logged.contains("mock stdout: starting"),
        "stdout not captured: {logged:?}"
    );
    assert!(
        logged.contains("mock stderr: noise"),
        "stderr not captured: {logged:?}"
    );
}

#[test]
fn spawn_allocates_free_port() {
    let sidecar = Sidecar::spawn(
        &mock_binary(),
        &fake_model(),
        &scratch_log("free-port"),
        SidecarRuntime::default(),
    )
    .expect("spawn mock sidecar");

    assert_ne!(sidecar.port(), 0, "port should be allocated");

    // The mock binds the port it was passed; connecting proves the port was
    // free and actually reached the child via --port.
    let addr = format!("127.0.0.1:{}", sidecar.port());
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match TcpStream::connect(&addr) {
            Ok(_) => break,
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => panic!("mock sidecar never bound {addr}: {e}"),
        }
    }
}

/// The mock scripts ignore `-m`, but `spawn` insists on a model path
/// — a server started without one loads nothing and answers every
/// completion with a 400, which is exactly the trap this signature
/// exists to close.
fn fake_model() -> PathBuf {
    PathBuf::from("models/not-a-real-model.gguf")
}

/// The child's argv, once it has one.
///
/// Straight after spawn the child may not have finished exec: ps then
/// shows "(bash)" with no arguments yet. Poll until `settled` appears —
/// the mocks park, so once visible the argv stays visible.
fn child_argv(pid: u32, settled: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let ps = std::process::Command::new("ps")
            .args(["-o", "args=", "-p", &pid.to_string()])
            .output()
            .expect("run ps");
        let argv = String::from_utf8_lossy(&ps.stdout).into_owned();
        if argv.contains(settled) || Instant::now() >= deadline {
            break argv;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn spawn_caps_inference_threads() {
    let sidecar = Sidecar::spawn(
        &mock_binary(),
        &fake_model(),
        &scratch_log("threads"),
        SidecarRuntime::default(),
    )
    .expect("spawn mock sidecar");

    let argv = child_argv(sidecar.pid(), "--threads");
    let expected = format!(
        "--threads {}",
        runner::sidecar::quiet_threads(num_cpus::get_physical())
    );
    assert!(
        argv.contains(&expected),
        "child argv missing '{expected}': {argv}"
    );
}

#[test]
fn the_sidecar_runs_at_the_context_it_reports() {
    // #251. `DEFAULT_CONTEXT` was written into every eval report's
    // `ModelInfo.context` and passed to llama-server nowhere, so the
    // server used its own defaults — a real run's log shows n_slots = 4
    // and n_ctx_slot = 32768, i.e. 131,072 tokens of KV cache against a
    // report claiming 8192. Every baseline on disk therefore records a
    // context its run did not use, which makes its `peak_rss` figure
    // incomparable with any run that honours the recorded one.
    //
    // The rule this pins: what `spawn` is told is what the child is
    // told. Not that 8192 is the right number — that question moves
    // scores and belongs to #232, behind a `--baseline` comparison.
    let runtime = SidecarRuntime {
        context: 4096,
        parallel: 2,
        ..SidecarRuntime::default()
    };
    let sidecar = Sidecar::spawn(
        &mock_binary(),
        &fake_model(),
        &scratch_log("context"),
        runtime,
    )
    .expect("spawn mock sidecar");

    let argv = child_argv(sidecar.pid(), "-c");
    for arg in runtime.args().chunks(2) {
        let expected = arg.join(" ");
        assert!(
            argv.contains(&expected),
            "child argv missing '{expected}': {argv}"
        );
    }
}

#[test]
fn becomes_ready_when_health_reports_ok() {
    let mut sidecar = Sidecar::spawn(
        &mock_binary(),
        &fake_model(),
        &scratch_log("ready"),
        SidecarRuntime::default(),
    )
    .expect("spawn mock sidecar");

    // The mock delays startup, so this must poll through connection-refused
    // before /health answers 200.
    sidecar
        .wait_until_ready(Duration::from_secs(10))
        .expect("sidecar became ready");
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn peak_rss_is_sampled_from_the_sidecar_process() {
    let mut sidecar = Sidecar::spawn(
        &mock_binary(),
        &fake_model(),
        &scratch_log("peak-rss"),
        SidecarRuntime::default(),
    )
    .expect("spawn mock sidecar");
    sidecar
        .wait_until_ready(Duration::from_secs(10))
        .expect("sidecar became ready");

    assert!(
        sidecar.peak_rss_mb() > 0,
        "a live sidecar should have a measurable resident set"
    );
}

#[test]
fn never_ready_times_out() {
    let mock = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/mock_bin/llama-server-mock-never-ready.sh");
    let mut sidecar = Sidecar::spawn(
        &mock,
        &fake_model(),
        &scratch_log("never-ready"),
        SidecarRuntime::default(),
    )
    .expect("spawn never-ready mock");

    let err = sidecar
        .wait_until_ready(Duration::from_millis(300))
        .expect_err("a sidecar that never serves /health must time out");
    assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
}

/// A model that died on the way up is not a slow one (#111). The app
/// already knew this and polled `has_exited` around its own wait loop;
/// the eval path did not, so a mislinked llama-server on a Pi 5 — dead
/// in milliseconds, before it bound a port — was reported as "not ready
/// within 300s", blaming the weights for a dynamic-linker failure.
///
/// Timing is part of the assertion: "not a timeout" is satisfied by an
/// error raised at second 299, which would miss the point entirely.
#[test]
fn a_sidecar_that_dies_during_load_fails_fast() {
    let mock = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/mock_bin/llama-server-mock-dies-on-load.sh");
    let mut sidecar = Sidecar::spawn(
        &mock,
        &fake_model(),
        &scratch_log("dies-on-load"),
        SidecarRuntime::default(),
    )
    .expect("spawn dying mock");

    let started = Instant::now();
    let err = sidecar
        .wait_until_ready(Duration::from_secs(30))
        .expect_err("a sidecar whose process exited must never report ready");
    let took = started.elapsed();

    assert_ne!(
        err.kind(),
        std::io::ErrorKind::TimedOut,
        "a dead sidecar reported as a timeout sends the reader to the model: {err}"
    );
    assert!(
        took < Duration::from_secs(5),
        "took {took:?} to notice a process that exited immediately"
    );
}

#[test]
fn killed_on_drop() {
    let sidecar = Sidecar::spawn(
        &mock_binary(),
        &fake_model(),
        &scratch_log("killed"),
        SidecarRuntime::default(),
    )
    .expect("spawn mock sidecar");
    let pid = sidecar.pid();

    drop(sidecar);

    // kill -0 probes for existence without signalling; success means the
    // child outlived its Sidecar.
    let alive = std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status()
        .expect("run kill -0")
        .success();
    assert!(!alive, "sidecar child {pid} survived drop");
}

/// #74: a llama-server upgrade can change grammar-sampling behaviour
/// while the weights are byte-identical, so an eval report has to be
/// able to say which sidecar produced it.
#[test]
fn version_reads_the_binarys_own_version() {
    let version = runner::sidecar::version(&mock_binary()).expect("read the mock's version");

    assert_eq!(version, "9999 (mock0000)");
}

/// The vendored sidecar directory in this checkout, or `None` when
/// nobody has run `scripts/vendor-sidecar.sh` — gitignored, like
/// libpdfium, so it skips quietly rather than failing on a fresh clone.
///
/// Gated with its callers: the two checks below read Mach-O load
/// commands through `otool`, so they are macOS-only, and CI runs on
/// Linux with `-D warnings`.
#[cfg(target_os = "macos")]
fn vendored_sidecar() -> Option<PathBuf> {
    let binary =
        runner::sidecar::binary_in(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sidecars"));
    binary.is_file().then_some(binary)
}

/// #50: an installer that ships llama-server must ship everything
/// llama-server needs.
///
/// This is the bug that shipped and was not caught: `sidecars/` held a
/// symlink to Homebrew's `llama-server`, a 33KB launcher against
/// `/opt/homebrew/opt/ggml/lib/libggml.0.dylib` and friends. It was
/// bundled correctly, kept its executable bit, and was Mach-O arm64 —
/// every check the packaging work had. It could not have started on any
/// machine that had not already installed llama.cpp, and the one
/// machine it was tested on had.
///
/// So the assertion is not "it runs here". It is that nothing it loads
/// comes from anywhere but its own directory or the OS — the only form
/// of the claim that a developer machine cannot accidentally satisfy.
#[cfg(target_os = "macos")]
#[test]
fn the_vendored_sidecar_loads_nothing_from_outside_its_own_directory() {
    let Some(binary) = vendored_sidecar() else {
        eprintln!("skipping: no vendored sidecar — see sidecars/README.md");
        return;
    };
    let dir = binary.parent().expect("a platform directory").to_owned();

    // Every Mach-O that ships beside it, not just the launcher: a
    // dependency reaching outside is just as fatal one level down.
    let mach_o = std::fs::read_dir(&dir)
        .expect("read the vendored sidecar directory")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| {
            std::fs::read(path)
                .ok()
                .and_then(|bytes| {
                    bytes
                        .get(..4)
                        .map(|magic| magic == [0xcf, 0xfa, 0xed, 0xfe])
                })
                .unwrap_or(false)
        });

    for object in mach_o {
        let otool = std::process::Command::new("/usr/bin/otool")
            .arg("-L")
            .arg(&object)
            .output()
            .expect("run otool");
        let listing = String::from_utf8_lossy(&otool.stdout);

        for line in listing.lines().skip(1) {
            let loaded = line.split_whitespace().next().unwrap_or_default();
            let ok = loaded.starts_with("@rpath/")
                || loaded.starts_with("@loader_path/")
                || loaded.starts_with("@executable_path/")
                || loaded.starts_with("/usr/lib/")
                || loaded.starts_with("/System/");
            assert!(
                ok,
                "{} loads {loaded}, which is not in its own directory and not part of macOS \
                 — an installed Kettle would not find it",
                object.display(),
            );
        }
    }
}

/// ...and the sibling half of the same claim: the launcher resolves
/// `@rpath` through `@loader_path`, so the dylibs beside it are the
/// ones it finds. Without this run-path entry the closure above is
/// present and still unreachable.
#[cfg(target_os = "macos")]
#[test]
fn the_vendored_sidecar_finds_its_dylibs_beside_itself() {
    let Some(binary) = vendored_sidecar() else {
        eprintln!("skipping: no vendored sidecar — see sidecars/README.md");
        return;
    };

    let otool = std::process::Command::new("/usr/bin/otool")
        .args(["-l"])
        .arg(&binary)
        .output()
        .expect("run otool");
    let listing = String::from_utf8_lossy(&otool.stdout);

    assert!(
        listing
            .lines()
            .filter_map(|line| line.trim().strip_prefix("path "))
            .any(|path| path.starts_with("@loader_path")),
        "{} has no @loader_path run path, so it cannot find the dylibs shipped beside it",
        binary.display(),
    );

    // And the whole thing actually loads: dyld resolving the closure is
    // the behaviour the two structural checks are proxies for.
    runner::sidecar::version(&binary).expect("the vendored sidecar starts and names itself");
}

/// A binary that isn't there is a plain error, not a panic and not a
/// silent "unknown" — an eval that cannot say what it ran should say so.
#[test]
fn version_of_a_missing_binary_is_an_error() {
    let missing = Path::new("/nonexistent/llama-server");

    runner::sidecar::version(missing)
        .expect_err("a sidecar binary that isn't there has no version to read");
}

/// #250: validation belongs before publication. The old vendor script
/// populated sidecars/<platform> and only then tried `--version`, so a
/// linker failure correctly stopped the script but left a broken bundle
/// for the next eval to discover.
#[cfg(unix)]
#[test]
fn a_sidecar_that_cannot_start_is_never_published() {
    use std::os::unix::fs::PermissionsExt;

    let root =
        std::env::temp_dir().join(format!("kettle-vendor-sidecar-test-{}", std::process::id()));
    let source = root.join("source");
    let destination = root.join("sidecars/linux-arm64");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&source).expect("create fake release");
    std::fs::write(source.join("llama-server"), "#!/bin/sh\nexit 127\n")
        .expect("write broken sidecar");
    std::fs::set_permissions(
        source.join("llama-server"),
        std::fs::Permissions::from_mode(0o755),
    )
    .expect("make fake sidecar executable");
    std::fs::write(source.join("libggml.so"), "not a real library")
        .expect("write fake runtime library");
    std::fs::write(source.join("LICENSE"), "MIT").expect("write fake licence");

    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/publish-sidecar.sh");
    assert!(
        script.is_file(),
        "the vendor path must use a separately testable publisher"
    );
    let status = std::process::Command::new("bash")
        .arg(script)
        .args(["linux-arm64"])
        .arg(&source)
        .arg(&destination)
        .arg("b-test")
        .status()
        .expect("run sidecar publisher");

    assert!(!status.success(), "the broken sidecar should be refused");
    assert!(
        !destination.exists(),
        "a failed validation published {}",
        destination.display()
    );

    std::fs::remove_dir_all(root).expect("remove sidecar publishing fixture");
}

#[cfg(unix)]
#[test]
fn a_sidecar_that_starts_is_published_with_its_provenance() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!(
        "kettle-vendor-sidecar-green-test-{}",
        std::process::id()
    ));
    let source = root.join("source");
    let destination = root.join("sidecars/linux-arm64");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&source).expect("create fake release");
    std::fs::write(
        source.join("llama-server"),
        "#!/bin/sh\nprintf '%s\\n' 'version: 10145 (test)'\n",
    )
    .expect("write working sidecar");
    std::fs::set_permissions(
        source.join("llama-server"),
        std::fs::Permissions::from_mode(0o755),
    )
    .expect("make fake sidecar executable");
    std::fs::write(source.join("libggml.so"), "not a real library")
        .expect("write fake runtime library");
    std::fs::write(source.join("LICENSE"), "MIT").expect("write fake licence");

    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/publish-sidecar.sh");
    let status = std::process::Command::new("bash")
        .arg(script)
        .args(["linux-arm64"])
        .arg(&source)
        .arg(&destination)
        .arg("b-test")
        .status()
        .expect("run sidecar publisher");

    assert!(status.success(), "the working sidecar should be published");
    assert!(destination.join("llama-server").is_file());
    assert!(destination.join("libggml.so").is_file());
    assert_eq!(
        std::fs::read_to_string(destination.join("BUILD")).expect("read build provenance"),
        "b-test\n"
    );

    std::fs::remove_dir_all(root).expect("remove sidecar publishing fixture");
}

#[test]
fn the_sidecar_is_told_not_to_think() {
    // #232. llama-server defaults `--reasoning` to `auto` (detect from
    // the template) and `--reasoning-budget` to -1 (unrestricted), so
    // Kettle was running every model under a policy nobody chose.
    // Gemma 4 took it up: 1,700–2,300 hidden tokens before each compact
    // answer, and 10m56s for two fixtures against 21.7s for Qwen2.5 7B
    // on the same machine.
    //
    // Kettle's model never reasons and never does maths (CLAUDE.md). It
    // answers small closed questions, so thinking buys nothing and costs
    // an order of magnitude — and, unrecorded, it makes two models'
    // timings incomparable for a reason the provenance cannot show.
    let runtime = SidecarRuntime::default();
    let sidecar = Sidecar::spawn(
        &mock_binary(),
        &fake_model(),
        &scratch_log("reasoning"),
        runtime,
    )
    .expect("spawn mock sidecar");

    let argv = child_argv(sidecar.pid(), "--reasoning");
    for expected in [
        "--reasoning off",
        "--reasoning-budget 0",
        // Belt and braces, and the reassuring name is the wrong one:
        // `none` leaves thoughts *unparsed in message.content*, which
        // is the field Rust validates against the pack's schema. A
        // model that thought anyway would fail every batch into review.
        // `deepseek` diverts them to `reasoning_content` and leaves the
        // answer clean.
        "--reasoning-format deepseek",
    ] {
        assert!(
            argv.contains(expected),
            "child argv missing '{expected}': {argv}"
        );
    }
}
