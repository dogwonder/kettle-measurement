//! llama-server sidecar management: spawn on a free port, poll /health
//! until ready, and guarantee the child dies with its `Sidecar`.

use std::fs::File;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const RSS_POLL: Duration = Duration::from_secs(1);

/// Context window, in tokens, a sidecar is spawned with unless a model
/// spec names another. Lives here rather than in the eval CLI because
/// the default and the flag that carries it must be one fact: the bug
/// in #251 was precisely that they were two, and the one that was
/// reported was not the one that ran.
pub const DEFAULT_CONTEXT: u32 = 8192;

/// Concurrent slots. One, deliberately: Kettle batches its questions
/// and awaits each batch, so extra slots buy nothing and multiply the
/// KV cache by their count. llama-server's own default of 4 is what
/// turned a claimed 8192-token context into 131,072 tokens of KV
/// allocation — invisible on a Mac, decisive on an 8GB Pi.
pub const DEFAULT_PARALLEL: u32 = 1;

/// The runtime policy a sidecar actually runs under (#251, #232).
///
/// This exists as a type rather than as more arguments to `spawn`
/// because it is the thing eval provenance claims. A baseline records
/// `scoring_version`, `recorded_at` and the server it ran on so a score
/// that moves can be attributed (CLAUDE.md); context belongs in that
/// set, and it is only worth recording if what is recorded is what was
/// passed. Every field here must reach the child's argv.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SidecarRuntime {
    pub context: u32,
    pub parallel: u32,
    pub reasoning: Reasoning,
}

/// Whether the model may think before it answers (#232).
///
/// llama-server's default is `auto` — detect from the model's own chat
/// template — with an *unrestricted* token budget. That is a policy
/// nobody chose, and reasoning-first families take it up: Gemma 4 spent
/// 1,700–2,300 hidden tokens before each compact answer, 10m56s for two
/// fixtures against 21.7s for Qwen2.5 7B on the same machine.
///
/// Kettle's model never reasons and never does maths (CLAUDE.md). It
/// answers small closed questions from a schema, so thinking buys
/// nothing and costs an order of magnitude — and, left implicit, it
/// makes two models' timings incomparable for a reason no baseline can
/// show. `Off` is the deliberate default; the variant exists so a
/// future pack can decide otherwise *and record it*.
///
/// Serialised in lowercase — the same word the `--reasoning` flag
/// carries, so the recorded policy and the argv read as one fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Reasoning {
    Off,
    On,
    /// Whatever the model's template wants — llama-server's default,
    /// and never Kettle's choice. Kept because "what did this run
    /// actually do" needs a way to say "we didn't decide".
    Auto,
}

impl Reasoning {
    /// The word the `--reasoning` flag carries — also what the recorded
    /// runtime policy prints, so the record and the argv share one
    /// spelling.
    pub fn as_flag(self) -> &'static str {
        match self {
            Reasoning::Off => "off",
            Reasoning::On => "on",
            Reasoning::Auto => "auto",
        }
    }
}

impl Default for SidecarRuntime {
    fn default() -> Self {
        SidecarRuntime {
            context: DEFAULT_CONTEXT,
            parallel: DEFAULT_PARALLEL,
            reasoning: Reasoning::Off,
        }
    }
}

impl SidecarRuntime {
    /// The flags this policy becomes. One place, so a test can assert
    /// on the same list the spawn uses.
    pub fn args(&self) -> Vec<String> {
        let mut args = vec![
            "-c".to_owned(),
            self.context.to_string(),
            "--parallel".to_owned(),
            self.parallel.to_string(),
            "--reasoning".to_owned(),
            self.reasoning.as_flag().to_owned(),
            "--reasoning-budget".to_owned(),
            match self.reasoning {
                Reasoning::Off => "0".to_owned(),
                _ => "-1".to_owned(),
            },
        ];
        // Pinned rather than left at `auto`, and `none` is the wrong
        // answer despite the name: it leaves thoughts *unparsed in
        // `message.content`* — the field Rust validates against the
        // pack's schema. A model that thought anyway would fail every
        // batch into review, which is a silent quality collapse caused
        // by somebody else's default. `deepseek` diverts thoughts to
        // `reasoning_content` and leaves the answer clean.
        args.extend(["--reasoning-format".to_owned(), "deepseek".to_owned()]);
        args
    }
}

/// This platform's vendored sidecar, relative to a directory holding
/// sidecars — e.g. `macos-arm64/llama-server` (sidecars/README.md).
///
/// A directory, not a file, and that is the fix rather than tidying:
/// llama-server is a 33KB launcher against ten `@rpath` dylibs, and it
/// resolves them through `LC_RPATH = @loader_path` — the directory it
/// sits in. Vendoring the executable alone produced a bundle that ran
/// only on a machine that already had llama.cpp installed somewhere
/// else, which is the one machine packaging must not be tested on
/// (#50). The siblings travel with it, so the name says "directory".
pub fn relative_binary() -> PathBuf {
    let name = if cfg!(target_os = "windows") {
        "llama-server.exe"
    } else {
        "llama-server"
    };
    Path::new(&platform_dir(std::env::consts::OS, std::env::consts::ARCH)).join(name)
}

/// `<os>-<arch>`, e.g. `macos-arm64` or `linux-arm64`.
///
/// A pure function of the two facts it needs, because the interesting
/// case cannot be built on the machine that writes it: Linux used to be
/// hardcoded to `linux-x86_64` for every non-macOS, non-Windows target,
/// so an aarch64 build — a Raspberry Pi, an ARM server — went looking
/// for an x86 binary. Taking the strings as arguments is what lets a
/// macOS host test the Linux answer.
fn platform_dir(os: &str, arch: &str) -> String {
    // `std::env::consts::OS` already says macos / linux / windows, so
    // the os needs no translation. Only the architecture does: Rust
    // says `aarch64` where the rest of the world, and llama.cpp's own
    // release assets, say `arm64`.
    let arch = match arch {
        "aarch64" => "arm64",
        other => other,
    };
    format!("{os}-{arch}")
}

/// The sidecar inside a given sidecars directory: the checkout's
/// `sidecars/` for the CLI, the bundle's `Contents/Resources/sidecars`
/// for the installed app.
pub fn binary_in(sidecars_dir: &Path) -> PathBuf {
    sidecars_dir.join(relative_binary())
}

/// A cloneable view of the highest resident set observed for one
/// sidecar process. The sampler starts with the child, so model loading
/// is included rather than only the later completion calls.
#[derive(Clone)]
pub struct PeakRss {
    pid: u32,
    kib: Arc<AtomicU64>,
}

impl PeakRss {
    pub fn megabytes(&self) -> u64 {
        if let Some(kib) = resident_set_kib(self.pid) {
            self.kib.fetch_max(kib, Ordering::Relaxed);
        }
        self.kib.load(Ordering::Relaxed).div_ceil(1_024)
    }
}

/// The log verbosity a sidecar is spawned at (#490).
///
/// Level 4 ("trace") rather than llama-server's default of 3, because
/// the one startup fact eval provenance needs — which device the model
/// loaded on — prints at 4 and stays silent at 3. A CUDA build and a
/// CPU-only build on the same box are different instruments, and below
/// this level their logs are byte-identical. The cost is per-request
/// trace lines in a log that is already disposable; the alternative was
/// recording the host's inventory, which is the wrong fact (a 5090 in
/// the box says nothing about whether llama-server used it).
const LOG_VERBOSITY: u32 = 4;

pub struct Sidecar {
    child: Child,
    port: u16,
    /// Where the child's startup output landed, so what it said about
    /// itself can be read back after it is ready (#490).
    log: PathBuf,
    peak_rss: PeakRss,
    stop_rss: mpsc::Sender<()>,
    rss_sampler: Option<JoinHandle<()>>,
}

impl Sidecar {
    /// Spawn the given llama-server binary serving `model`, on a freshly
    /// allocated free port, capturing its stdout and stderr to `log`.
    ///
    /// `model` is not optional, and that is the whole point. Without
    /// `-m`, llama-server starts in "router mode": it listens, it
    /// answers `/health` with a cheerful 200, and it has no model
    /// loaded — so every completion comes back 400 "model not found".
    /// A server that is up but cannot answer is worse than one that
    /// failed to start, because everything downstream reports healthy.
    ///
    /// `runtime` is not optional for the same reason `model` is not:
    /// leaving it to llama-server's defaults is what produced a
    /// baseline claiming a context its run never used (#251).
    pub fn spawn(
        binary: &Path,
        model: &Path,
        log: &Path,
        runtime: SidecarRuntime,
    ) -> io::Result<Self> {
        let port = free_port()?;
        let log_file = File::create(log)?;
        let child = Command::new(binary)
            .arg("--port")
            .arg(port.to_string())
            .arg("-m")
            .arg(model)
            .arg("--threads")
            .arg(quiet_threads(num_cpus::get_physical()).to_string())
            // Loud enough to name the device it loads on (#490); see
            // LOG_VERBOSITY. Host mechanics like --threads rather than
            // runtime policy: it changes what the log says, never what
            // the model computes, so it does not belong in args().
            .arg("-lv")
            .arg(LOG_VERBOSITY.to_string())
            // The runtime policy goes on the argv from `args()` rather
            // than being spelled out here, so the flags a test asserts
            // on and the flags the child receives cannot drift apart —
            // two copies of that fact is how #251 happened.
            .args(runtime.args())
            .stdout(log_file.try_clone()?)
            .stderr(log_file)
            .spawn()?;
        let peak_rss = PeakRss {
            pid: child.id(),
            kib: Arc::new(AtomicU64::new(0)),
        };
        let (stop_rss, stop_requested) = mpsc::channel();
        let rss_sampler = {
            let peak = peak_rss.clone();
            let pid = child.id();
            std::thread::spawn(move || loop {
                if let Some(kib) = resident_set_kib(pid) {
                    peak.kib.fetch_max(kib, Ordering::Relaxed);
                }
                match stop_requested.recv_timeout(RSS_POLL) {
                    Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
            })
        };
        Ok(Self {
            child,
            port,
            log: log.to_path_buf(),
            peak_rss,
            stop_rss,
            rss_sampler: Some(rss_sampler),
        })
    }

    /// What this sidecar loaded the model on, in its own startup words
    /// (#490) — e.g. `"MTL0 (Apple M1 Pro)"`, or `"CPU"` for a build
    /// with no accelerator to offer. `None` when the log cannot be read
    /// or does not say: not recorded, never "no accelerator".
    ///
    /// Ask it after [`Sidecar::wait_until_ready`] — the device is named
    /// during model load, so a sidecar that is ready has already said.
    pub fn device(&self) -> Option<String> {
        device_from_startup(&std::fs::read_to_string(&self.log).ok()?)
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Whether the model process has already stopped (#111).
    ///
    /// Waiting for a model to load is the one place a person is asked
    /// to sit still for minutes, so the caller has to be able to tell
    /// "still loading" from "died on the way up" — a binary the OS
    /// refused to run, a half-finished install, weights it choked on.
    /// Without this, a dead sidecar is indistinguishable from a slow
    /// one until the load timeout expires, and the person watches a
    /// progress bar that was never going to move.
    ///
    /// `false` if the answer can't be had: never claim a process is
    /// gone on the strength of a failed question.
    pub fn has_exited(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(Some(_)))
    }

    pub fn peak_rss(&self) -> PeakRss {
        self.peak_rss.clone()
    }

    pub fn peak_rss_mb(&self) -> u64 {
        self.peak_rss.megabytes()
    }

    /// Poll `/health` until it answers 200, or fail with `TimedOut`.
    ///
    /// A process that has already exited fails immediately instead, and
    /// says so — waiting out a five-minute load timeout for a sidecar
    /// that died in milliseconds reports a linker failure as a slow
    /// model. That distinction is the whole reason `has_exited` exists;
    /// keeping it here means every caller gets it, rather than each
    /// wait loop having to remember.
    pub fn wait_until_ready(&mut self, timeout: Duration) -> io::Result<()> {
        let deadline = Instant::now() + timeout;
        loop {
            if health_ok(self.port) {
                return Ok(());
            }
            // Asked after /health, not before: a sidecar that answered
            // and then exited within the same tick is ready, and the
            // work it did should not be thrown away on the ordering.
            if let Ok(Some(status)) = self.child.try_wait() {
                return Err(io::Error::other(format!(
                    "sidecar exited during load ({status}) without serving /health"
                )));
            }
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("sidecar on port {} not ready within {timeout:?}", self.port),
                ));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for Sidecar {
    fn drop(&mut self) {
        let _ = self.stop_rss.send(());
        if let Some(sampler) = self.rss_sampler.take() {
            let _ = sampler.join();
        }
        // Best-effort: the child may already have exited. Waiting reaps the
        // process so the pid is gone, not a zombie, when drop returns.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(target_os = "linux")]
fn resident_set_kib(pid: u32) -> Option<u64> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    status.lines().find_map(|line| {
        line.strip_prefix("VmRSS:")?
            .split_whitespace()
            .next()?
            .parse()
            .ok()
    })
}

#[cfg(target_os = "macos")]
fn resident_set_kib(pid: u32) -> Option<u64> {
    let output = Command::new("/bin/ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout).trim().parse().ok()
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn resident_set_kib(_pid: u32) -> Option<u64> {
    None
}

/// One GET /health probe; any failure (refused, hung up, non-200) is
/// simply "not ready yet".
fn health_ok(port: u16) -> bool {
    let probe = || -> io::Result<String> {
        let mut stream = TcpStream::connect(("127.0.0.1", port))?;
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        write!(
            stream,
            "GET /health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
        )?;
        let mut response = String::new();
        stream.read_to_string(&mut response)?;
        Ok(response)
    };
    probe().is_ok_and(|response| {
        response.lines().next().is_some_and(|status_line| {
            status_line.starts_with("HTTP/1.")
                && status_line.split_whitespace().nth(1) == Some("200")
        })
    })
}

/// Ask the OS for a free port by binding port 0, then release it for the
/// child to claim.
fn free_port() -> io::Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}

/// Quiet-mode inference thread cap: half the physical cores, at least one
/// (brief §"quiet operation").
pub fn quiet_threads(physical_cores: usize) -> usize {
    (physical_cores / 2).max(1)
}

/// What `llama-server --version` says about itself, e.g.
/// `"10050 (b15ca938a)"` (#74).
///
/// The one drift channel a pinned local GGUF doesn't close: a sidecar
/// upgrade can change how `json_schema` sampling behaves while the
/// weights are byte-identical, so a score that moved after one reads as
/// "the prompt edit broke it" unless the report can name the sidecar
/// too.
///
/// llama-server prints two lines to stderr and exits 0:
///
/// ```text
/// version: 10050 (b15ca938a)
/// built with AppleClang 21.0.0.21000099 for Darwin arm64
/// ```
///
/// The build and commit are what identify it; the compiler line is
/// noise. Both streams are read because "which stream does it use"
/// is exactly the kind of thing a version bump changes.
pub fn version(binary: &Path) -> io::Result<String> {
    let output = Command::new(binary).arg("--version").output()?;
    let said = String::from_utf8_lossy(&output.stderr).into_owned()
        + &String::from_utf8_lossy(&output.stdout);

    said.lines()
        .find_map(|line| line.trim().strip_prefix("version:"))
        .map(|version| version.trim().to_owned())
        .filter(|version| !version.is_empty())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{} did not say what version it is", binary.display()),
            )
        })
}

/// What llama-server loaded the model on, read from its startup output
/// (#490).
///
/// The fact a measurement needs is not what the host has installed —
/// a CUDA build and a CPU-only build on the same box would record
/// byte-identical host inventory while being different instruments
/// (`scripts/vendor-sidecar.sh` falls back to a CPU-only build when
/// `nvcc` is absent, and the warning is easy to miss). The sidecar
/// itself names what it prepared, once per device, during model load:
///
/// ```text
/// 0.00.532.412 I llama_prepare_model_devices: using device MTL0 (Apple M1 Pro) (unknown id) - 25558 MiB free
/// ```
///
/// The device is taken in the sidecar's own words, shorn of the free-
/// memory suffix and the `(unknown id)` shrug. A build with nothing to
/// offload to prepares no device and prints no such line — so "no
/// `using device` line" only means CPU when the output proves the
/// sidecar was speaking at a level that names devices at all, which its
/// `device_info:` table (printed at the same level, before load) does.
/// Anything less says nothing, and `None` means exactly that: not
/// recorded, never "no accelerator".
pub fn device_from_startup(startup: &str) -> Option<String> {
    let devices: Vec<&str> = startup
        .lines()
        .filter_map(|line| {
            let named = line.split("using device ").nth(1)?;
            let named = named.split(" - ").next().unwrap_or(named).trim();
            let named = named.strip_suffix("(unknown id)").unwrap_or(named).trim();
            (!named.is_empty()).then_some(named)
        })
        .collect();
    if !devices.is_empty() {
        return Some(devices.join(", "));
    }
    startup
        .lines()
        .any(|line| line.trim_end().ends_with("device_info:"))
        .then(|| "CPU".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The vendored sidecar is a directory, and the tests that matter
    /// are about the two halves of that: it is nested under a platform
    /// directory, and the executable is plainly named. A flat
    /// `llama-server-macos-arm64` said "one file, move it where you
    /// like", which was wrong — it cannot start without its siblings.
    #[test]
    fn the_sidecar_binary_lives_in_a_platform_directory() {
        let relative = relative_binary();
        let mut parts = relative.iter();

        let platform = parts.next().expect("a platform directory");
        assert!(
            platform.to_string_lossy().contains('-'),
            "the platform directory names an os and an arch, got {platform:?}",
        );
        assert!(
            parts
                .next()
                .expect("an executable")
                .to_string_lossy()
                .starts_with("llama-server"),
            "the executable is llama-server, got {relative:?}",
        );
        assert_eq!(parts.next(), None, "nothing deeper than platform/binary");
    }

    /// The platform directory has to name the architecture this build
    /// actually targets, not the one its operating system usually runs
    /// on.
    ///
    /// Linux mapped to `linux-x86_64` for every non-macOS, non-Windows
    /// target regardless of arch, so an aarch64 build — a Raspberry Pi,
    /// an ARM server — went looking for an x86 binary. macOS was right
    /// only by accident: Kettle's macOS target happens to be arm64.
    ///
    /// Asserted for every platform rather than the one running the
    /// test, because the broken case cannot be compiled on the machine
    /// that fixed it.
    #[test]
    fn the_platform_directory_names_the_architecture_it_was_built_for() {
        assert_eq!(platform_dir("linux", "aarch64"), "linux-arm64");
        assert_eq!(platform_dir("linux", "x86_64"), "linux-x86_64");
        assert_eq!(platform_dir("macos", "aarch64"), "macos-arm64");
        assert_eq!(platform_dir("macos", "x86_64"), "macos-x86_64");
        assert_eq!(platform_dir("windows", "x86_64"), "windows-x86_64");
    }

    /// ...and the running build agrees with that table, which is what
    /// ties the pure function to the paths this process will use.
    #[test]
    fn this_build_resolves_its_own_platform() {
        let relative = relative_binary();
        let platform = relative
            .iter()
            .next()
            .expect("a platform directory")
            .to_string_lossy()
            .into_owned();

        assert_eq!(
            platform,
            platform_dir(std::env::consts::OS, std::env::consts::ARCH),
        );
    }

    /// Every consumer resolves the sidecar the same way — the CLI
    /// against `sidecars/`, the app against its bundle — so the shape
    /// lives here rather than being spelled out in each of them.
    #[test]
    fn the_sidecar_resolves_under_whatever_directory_holds_it() {
        assert_eq!(
            binary_in(Path::new(
                "/Applications/Kettle.app/Contents/Resources/sidecars"
            )),
            Path::new("/Applications/Kettle.app/Contents/Resources/sidecars")
                .join(relative_binary()),
        );
    }

    #[test]
    fn quiet_threads_is_half_physical_cores_min_one() {
        assert_eq!(quiet_threads(8), 4);
        assert_eq!(quiet_threads(6), 3);
        assert_eq!(quiet_threads(2), 1);
        assert_eq!(quiet_threads(1), 1);
    }

    /// The device is the sidecar's own words (#490), minus the noise
    /// that would make two runs on the same device read as different:
    /// the free-memory figure moves with what else is running, and
    /// `(unknown id)` says nothing at all. These lines are verbatim
    /// from the vendored b10145 on this project's own machine.
    #[test]
    fn the_device_is_read_from_the_using_device_line() {
        let startup = "\
0.00.070.254 I cmn  common_param: common_params_print_info: build 10145 (ad256ded3) with AppleClang 21.0.0.21000101 for Darwin arm64
0.00.070.257 I cmn  common_param: device_info:
0.00.070.265 I cmn  common_param:   - MTL0    : Apple M1 Pro (25559 MiB, 25558 MiB free)
0.00.070.265 I cmn  common_param:   - BLAS    : Accelerate (0 MiB, 0 MiB free)
0.00.532.412 I llama_prepare_model_devices: using device MTL0 (Apple M1 Pro) (unknown id) - 25558 MiB free
0.02.399.687 I srv  llama_server: model loaded
";
        assert_eq!(
            device_from_startup(startup).as_deref(),
            Some("MTL0 (Apple M1 Pro)"),
        );
    }

    /// Two GPUs are two `using device` lines; the record names both,
    /// because "which device produced this number" (#432) is not
    /// answered by naming half of them.
    #[test]
    fn every_prepared_device_is_recorded() {
        let startup = "\
0.00.061.002 I cmn  common_param: device_info:
0.00.612.001 I llama_prepare_model_devices: using device CUDA0 (NVIDIA GeForce RTX 5090) (unknown id) - 31331 MiB free
0.00.612.004 I llama_prepare_model_devices: using device CUDA1 (NVIDIA GeForce RTX 5090) (unknown id) - 31558 MiB free
";
        assert_eq!(
            device_from_startup(startup).as_deref(),
            Some("CUDA0 (NVIDIA GeForce RTX 5090), CUDA1 (NVIDIA GeForce RTX 5090)"),
        );
    }

    /// A CPU-only build prepares no device and prints no `using device`
    /// line — observed, not assumed: the same absence `-dev none`
    /// produces. Its `device_info:` table proves the sidecar was
    /// speaking at a level that names devices, so the silence is a
    /// recorded fact ("CPU"), not a gap.
    #[test]
    fn a_startup_that_names_no_device_at_a_level_that_would_is_a_cpu_run() {
        let startup = "\
0.00.057.089 I cmn  common_param: device_info:
0.00.057.093 I cmn  common_param:   - CPU     : INTEL(R) XEON(R) GOLD 6530 (773849 MiB, 773849 MiB free)
0.00.876.628 I load_tensors:   CPU_Mapped model buffer size =  2862.99 MiB
0.05.859.504 I srv  llama_server: model loaded
";
        assert_eq!(device_from_startup(startup).as_deref(), Some("CPU"));
    }

    /// A log from the default verbosity — every eval log before #490 —
    /// says nothing about devices, and the answer must be "not
    /// recorded" rather than a confident "CPU". These lines are what
    /// verbosity 3 actually prints.
    #[test]
    fn a_quiet_startup_records_nothing_rather_than_guessing_cpu() {
        let startup = "\
0.00.063.028 I cmn  common_param: common_params_print_info: verbosity = 3 (adjust with the `-lv N` CLI arg)
0.00.064.653 I srv    load_model: loading model 'weights.gguf'
0.01.082.035 I srv  llama_server: model loaded
";
        assert_eq!(device_from_startup(startup), None);
        assert_eq!(device_from_startup(""), None);
    }
}
