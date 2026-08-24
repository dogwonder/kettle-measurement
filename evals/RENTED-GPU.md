# Running an eval on a rented GPU

Written 11 August 2026, the day the first one was run, from what it cost
rather than from what was planned. A full letter run is ~2h on the M1
Pro; M8's comparisons want more of them than that budget allows, so a
rented box is worth having. Everything here is what the first attempt
hit, in the order it hit it.

`scripts/pod-eval.sh` automates the happy path. This file is why each
step is what it is, and what to do when it is not happy.

## Before anything: is it worth it?

The provisioning below took an afternoon the first time and should take
twenty minutes the second. If a run is ~30 minutes locally, it is not
worth renting anything. The letter bed (~2h, 794 fixtures) is; the
renewal bed (~30m, 54 fixtures) is not.

**Setup time is not recovered by a faster GPU.** Count the pull, the
CUDA build and the weights download before deciding.

## The privacy line

**Bed fixtures only, and this is not negotiable.** Every fixture in
`packs/*/fixtures/` is wholly synthetic and may travel. Nothing derived
from a real document ever may: no `*.private.*` input, no OCR or
transcript of one, no field-evidence run (#428). Those live inside
Kettle's deletion boundary, and a machine somebody else owns is exactly
where that boundary gets crossed by accident.

`pod-eval.sh` takes a pack id and offers no `--fixture-dir`, so no
argument exists through which a private path could reach the pod. Keep
it that way.

## Choosing the box

- **CUDA 12.8 or newer.** An RTX 5090 is Blackwell, compute capability
  12.0 (`sm_120`). 12.4 and below cannot target it: the build either
  refuses `compute_120` or produces a binary that dies at runtime with
  no kernel image for the device — after a ten-minute compile.
- **CUDA 13 is not "newer and therefore safer".** A pinned llama.cpp tag
  predates it and may not compile against it. 12.8/12.9 is the tested
  range.

- **A card that is not a 5090 needs `KETTLE_CUDA_ARCH`.**
  `vendor-sidecar.sh` builds with
  `-DCMAKE_CUDA_ARCHITECTURES=${KETTLE_CUDA_ARCH:-120}`, and 120 is
  `sm_120` — Blackwell, this file's original box. On a 3090 (Ampere,
  `sm_86`) or a 4090 (Ada, `sm_89`) that default compiles for an
  architecture the card does not have, and the binary dies at runtime
  with no kernel image for the device, after the full compile. Set
  `KETTLE_CUDA_ARCH=86` or `=89` before running anything. The default is
  not wrong, it is *specific*, and it silently encodes which box this
  playbook was written on (24 August 2026).
- **Pre-Blackwell cards widen the CUDA range rather than narrowing it.**
  The 12.8-or-newer rule above exists only for `sm_120`. Ampere and Ada
  build fine on 12.4 through 12.9, so a 3090 or 4090 is the *easier*
  box, not a compromise. Avoid CUDA 13 on any of them.
- **The GPU is chosen on memory bandwidth, not on compute or VRAM.** An
  eval is ~95% generation, which is bandwidth-bound; a 4B model in
  Q4_K_M occupies ~3.5GB, so 24GB is already excessive. Measured
  24 August: a 3090 runs the letter development bed in ~30 minutes a
  pass against the M1 Pro's 107, about 3.6x. The 5090's 9.1x (19 August)
  is the honest comparison for that card and not a general "pod" figure
  — the box is a variable, not a constant.
- **Take the provider's own template over a raw Docker Hub image.**
  `nvidia/cuda:*` pulls from Docker Hub, whose anonymous rate limit is
  shared per egress IP and is often already spent by another tenant on
  the same host. The signature is a pull that loops rather than
  progresses: the same two layer digests restarting, "still fetching"
  forever, no error. A provider-hosted image sidesteps it entirely.
  This reverses the intuition that a leaner image is better — cached
  and mirrored beats small.
- **A stuck initialise is a dead pod.** Give it 15 minutes, then
  terminate and start one in a different region. You are billed for the
  wait.
- **Disk: 50GB+.** The CUDA build tree is several GB of intermediates
  even pinned to one architecture, and `mktemp` puts it on `/tmp`, which
  on a container is usually the overlay and may be much smaller than the
  data volume. `export TMPDIR=/workspace/tmp` moves it.
  **Check which way round it is on your pod before following that.**
  On the 24 August 3090 the advice inverted: `/workspace` was a *network*
  mount (MooseFS) at 479 MB/s while the container overlay had 30GB free
  at 1.8 GB/s, so the build, the toolchain and `TMPDIR` all belonged on
  the overlay and only the weights on `/workspace`. Measure it —
  `dd if=/dev/zero of=<path>/.probe bs=1M count=512 conv=fsync` on each —
  rather than assuming the data volume is local. A network mount also
  makes `df` useless: it reported 929TB free on a volume with a quota,
  which is the reading `pod-eval.sh` refuses to trust.

## Credentials

The pod needs **read access to one private repository** and nothing
else. It never pushes; results come back as a tarball, and the relay
below carries that without SSH at all.

**Do not run `gh auth login`.** Its default scopes include `repo`, which
is read *and write* to every repository on the account, and it writes
the token in plaintext to a disk you do not own and cannot be sure is
wiped.

Use a **deploy key, generated on the pod**:

```sh
ssh-keygen -t ed25519 -f ~/.ssh/pod -N ""
cat ~/.ssh/pod.pub          # paste into the repo's Deploy keys, WRITE ACCESS OFF
GIT_SSH_COMMAND="ssh -i ~/.ssh/pod" git clone git@github.com:dogwonder/kettle.git
```

Generated there, the private half exists only there and dies with the
pod. Name the key for the machine (`runpod-5090-11aug`) so revoking the
right one later is obvious. Delete it from the repo when the pod goes.

The clone keeps working after the key is deleted — git needs
credentials only to reach the remote, and the eval reads only local
`.git` state. So deleting early costs nothing except the ability to
pull a fix, which is worth knowing before you delete it early.

## The weights

`app/src-tauri/models.json` pins every offered model with a URL, a
SHA-256 and a byte count. Download on the pod rather than uploading from
home — a datacentre pulls 3GB in a minute.

```sh
pip install -U "huggingface_hub[hf_transfer]"
export HF_HUB_ENABLE_HF_TRANSFER=1
hf download bartowski/Qwen_Qwen3.5-4B-GGUF Qwen_Qwen3.5-4B-Q4_K_M.gguf --local-dir models/
mv models/Qwen_Qwen3.5-4B-Q4_K_M.gguf models/qwen3.5-4b-q4_k_m.gguf
sha256sum models/qwen3.5-4b-q4_k_m.gguf
```

**Paste the download as one line.** Broken across lines without
trailing backslashes it runs as three commands, and the first —
`hf download <repo>` with no filename — asks for the *entire*
repository, every quantisation, tens of gigabytes. That is how the
13 August pod filled its volume before it had done anything. The tell is
`snapshot_download` in the traceback, above two "command not found"
lines that read as noise.

`--local-dir` matters: without it the real file goes to
`~/.cache/huggingface` behind a symlink. The rename matters because the
manifest and the runner use Kettle's name, not the publisher's.

**The digest is the load-bearing step.** The CLI verifies its own
transfer, which only proves you received what the repo serves today.
Our digest proves it is the same bytes the committed baselines were
measured on. Different guarantees, and the measurement depends on the
second. A mismatch means stop, not proceed.

If a model is *not* in `models.json`, its provenance can still be
recovered without downloading anything: Hugging Face serves an LFS
file's SHA-256 as its object id, so hash the local copy and match it
against `https://huggingface.co/api/models/<org>/<repo>/tree/main`
until a repository claims both digest and byte count. That is how
Qwen3.5-4B was identified for its `models.json` entry.

## Run it detached

**The eval is a child of the shell that started it.** Close the web
terminal, drop the connection, or let a laptop sleep, and the run dies
with no message anywhere — `pod-eval.log` simply stops. The first letter
run lost 326 of 794 fixtures that way.

```sh
tmux new -s eval
. "$HOME/.cargo/env"
cargo run --locked -p kettle -- eval <pack> --model <weights> \
  --resume --write-baseline pod-baseline.json 2>&1 | tee -a pod-eval.log
# Ctrl-B then D to detach; tmux attach -t eval to return
```

`--resume` reuses fixtures already scored under an identical key — same
model, pack version, bed digest and scoring version — so a killed run
picks up rather than starting over. Confirm it worked by watching
`ls evals/runs/run1 | wc -l` continue from where it stopped rather than
resetting.

A second pack on the same pod writes into that same `run1`, so count its
own directories — `ls evals/runs/run1 | grep -c renewal-diff` — rather
than the total, which still carries the first pack's.

The eval prints nothing until it finishes, so silence is not a symptom.
The signs of life are that count climbing and `nvidia-smi` showing
`llama-server` resident with real utilisation. The signs of death are
`ps aux | grep "[k]ettle eval"` returning nothing and the GPU dropping
to 0%.

## What a pod run can and cannot tell you

- **The score is the model's. The timing is the pod's.** Never merge a
  pod timing into `tiers.json` as a user-facing tier: that sentence is a
  claim about somebody's own laptop.
- **A pod is a different instrument.** `evals/README.md` says scores are
  machine-independent while timings are not — that is asserted, not
  measured, and a different backend (CUDA vs Metal) is exactly where it
  could fail. Until it *is* measured, do not use a cross-machine
  comparison as the sole evidence that a prompt edit changed nothing.
- **When the bed moved anyway, none of this bites**, because #320
  refuses the comparison regardless. That is why the first pod run was a
  bed-change run: there was no delta for the instrument change to
  contaminate.
- **The cheap experiment worth running once**: the same commit, bed and
  weights on both machines. That measures the instrument difference
  directly and would let `evals/README.md` stop asserting it.
- **It has now been run once, and the scores held** (24 August 2026).
  The letter development bed at scoring 15, same commit, same bed digest
  `57b37e87…`, same weights SHA-256, same pinned sidecar tag: M1 Pro
  (Metal, 23 August) against RTX 3090 (CUDA, 24 August). **All 56
  comparable extraction strata report identical precision on identical
  denominators.** Pooled obligations 1.00 (n=509), end-to-end 1.00,
  review 0%, both harm gates 0.00 at n=240 and n=101, containment 0
  escaped of 4,036 — on both machines. So on this bed, at this scoring
  version, score equivalence across backends is measured rather than
  assumed. It is one pack on one bed and not a general law, but it is
  no longer only a sentence.
- **The timings are not equivalent, and that half never was**: 23m15s a
  pass on the 3090 against 107m12s on the M1 Pro. Which is exactly why a
  pod timing must not become a `tiers.json` entry.

## Bringing it back

On the pod:

```sh
tar czf pod-run.tgz evals/runs/run1 pod-baseline-letter.json pod-eval.log
ls -lh pod-run.tgz
```

Tens of MB — the 355-fixture letter recording was 8.1MB.

Then get it off, by whichever route the pod was reached by.

**The relay is the reliable route, and worth reaching for first.** SSH
to a pod works only if your public key was in the provider's *account
settings before the pod was created* — that is when it gets injected. If
it was not, there is no fixing it on a running pod: `scp` falls back to
password auth, there is no password, and the prompt is the only thing
that tells you. That is a poor moment to discover it, with a finished
recording on a billed machine. So:

```sh
runpodctl send pod-run.tgz          # on the pod, prints a one-time code
runpodctl receive <code>            # on the receiving machine
```

No keys and no ports. Note the transfer goes through the provider's
relay, which is acceptable here for the same reason the run was: bed
fixtures are synthetic. A recording that contained anything from a real
document could not use this route, and could not have been made on a
rented box in the first place.

**With SSH configured.** The provider's Connect panel offers two things
that both look like SSH, and only one of them can carry a file:

- **SSH over exposed TCP** — `ssh root@203.0.113.7 -p 22022 -i ~/.ssh/id_ed25519`.
  A real host and a real port. This is the one that works.
- **The proxy** — `ssh <podid>-<hash>@ssh.runpod.io`. No exposed port,
  and `scp` over it does not reliably work. If this is the only option
  shown, the pod was not started with a public IP and no port will fix
  it: use the relay above.

Take the host, port and key from the first form and run this **on the
receiving machine**, not the pod:

```sh
scp -P 22022 -i ~/.ssh/id_ed25519 \
  root@203.0.113.7:/workspace/kettle/pod-run.tgz .
```

The address above is a documentation placeholder — read yours off the
Connect panel. Note `scp` takes `-P` for the port where `ssh` takes
`-p`; that difference has cost people more time than it has any right
to.

**Verify before destroying the pod:**

```sh
tar tzf pod-run.tgz | wc -l         # thousands of files
```

The run directory is the expensive artefact — it is what lets a score be
re-asked under new scoring with no GPU at all — and until the copy lands
it exists nowhere else. There is no resume across machines.

Archive per `CLAUDE.md`: a dated directory in the public
`dogwonder/kettle-runs` repo with a MANIFEST naming pack, model, eval
set, scoring version, bed digest, sidecar, machine and what it backs.
For a pod run, the manifest must also say **that it is a pod run**, on
what GPU, and — if the tree carried hand-applied patches — which commit
they correspond to.

## Known rough edges

Found by the first run, all in `scripts/`:

- `vendor-sidecar.sh` downloads with `curl -fsSL`, so a slow fetch is
  indistinguishable from a hung one. Wants `--progress-bar`.
- It re-downloads the source archive every run — no cache — so each
  retry pays for it again.
- **It deletes its temp directory on failure**, including a completed
  CUDA build. A publish-step error therefore costs the entire compile a
  second time. Cleanup is right on success and wrong on failure. This is
  the expensive one.
- The cargo environment is not in a fresh shell's profile, so
  `. "$HOME/.cargo/env"` is needed per shell until it is added to
  `~/.bashrc`.

Added by the fourth run (24 August, a 3090 rather than a 5090):

- **`vendor-sidecar.sh` decides on CUDA with `command -v nvcc`**, and
  RunPod's PyTorch images ship the toolkit at `/usr/local/cuda/bin`
  *without putting it on `PATH`*. So it takes the CPU branch and says
  so — "no nvcc found, building a CPU-only x86_64 sidecar; an eval on
  this will be correct and extremely slow" — on **stderr**. The script
  is right and its message is right; what fails is anyone watching a
  filtered log, because that sentence matches no failure word. Run
  `export PATH=/usr/local/cuda/bin:$PATH` first, and check the log says
  `building b10145 with CUDA=ON` before walking away. A CPU-only bed run
  on a rented GPU is the most expensive way to get correct answers.
- **`CUDA=ON` at build time does not prove the GPU ran the model.**
  Verify with `nvidia-smi` once fixtures are scoring: expect the model
  resident in VRAM (~3.5GB for 4B Q4_K_M) and utilisation in the high
  tens of percent. A reading of 0% taken during model load means
  nothing, so take two, a few seconds apart.

Added by the third run, both in `pod-eval.sh` itself:

- **It checks for a Rust toolchain only by using one**, at line 109,
  after the CUDA build. Every prerequisite it needs is knowable in the
  first second; one of them costs ten minutes to discover.
- **It runs the eval without `--resume`**, unlike the recipe above. A
  letter run that dies at fixture 700 therefore starts at zero if the
  script is re-run, which is the opposite of what a person reaching for
  the script at that moment wants.

Added by the fifth run (21 August), and this one is about the *check*
rather than the script around it:

- **The 2GB probe cannot fail on the failure it guards.** The disk
  section above is right that `df` lies on a network mount and that the
  check must be a write — but `dd` does ordinary writes, and `rust-lld`
  **mmaps** its output. On MooseFS (`mfs#…runpod.net`, which is what
  `/workspace` is) an mmap write can return `SIGBUS` with tens of
  gigabytes free. The probe passed, `du` showed 5.1G against a volume
  with room, and the link died anyway:

  ```
  collect2: fatal error: ld terminated with signal 7 [Bus error], core dumped
  error: could not compile `runner` (test "comparison")
  ```

  That is the same shape as this file's own closing lesson about checks
  that only ever run against synthetic input: a probe that exercises a
  different syscall from the thing it is standing in for reads as
  coverage and is not.

  **The fix is to keep the build off the network mount.** The container
  overlay is local, and on the 21 August pod it had 30G free against
  96M used:

  ```sh
  export CARGO_TARGET_DIR=/root/target TMPDIR=/root/tmp
  mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"
  ```

  `pod-eval.sh` sets `TMPDIR` with `: "${TMPDIR:=/workspace/tmp}"`, so
  an export survives it, and it sets `CARGO_PROFILE_DEV_DEBUG=0` itself,
  which matters more once `target/` is on the smaller disk. Leave
  `evals/runs` on `/workspace`: those are ordinary small writes, and it
  is the artefact you actually have to get off the box.

  Diagnosing it is two commands, and they separate the two causes that
  look identical from the log — quota exhaustion and mmap refusal:

  ```sh
  dd if=/dev/zero of=$TMPDIR/.probe bs=1M count=2048 status=none && echo OK || echo FULL
  df -h /root /
  ```

  `FULL` is the quota story this file already tells. `OK` plus a dead
  linker is this one.

- **Re-running costs the CUDA build again.** `vendor-sidecar.sh` has no
  skip logic, so a failure *after* vendoring — which the above is, since
  it dies in `cargo test` — pays ten minutes to rebuild a binary that is
  already sitting correct in `sidecars/`. Check before you assume the
  worst: `ls sidecars/linux-x86_64`. If the eval itself is all that is
  left, the direct recipe under "Run it detached" skips the script and
  starts immediately.

- **Paste one line at a time while `apt-get` is running.** In the web
  terminal, lines pasted during apt's output queue into *its* stdin
  rather than bash's and vanish silently. On 21 August that swallowed
  the whole rustup install, and the first symptom was `cargo: command
  not found` twenty minutes later. Related: a fresh shell — including
  every new tmux window — has none of the exports, and
  `. "$CARGO_HOME/env"` then expands to `/env`, which is the error
  `pod-eval.sh` already documents. The web terminal sources
  `.runpod-web-terminal.bashrc`, not the `.bashrc` the setup appends to,
  so setting the variables per shell is the reliable move rather than
  the profile.

## What the first run cost, so the second does not

Four defects, each waiting behind the last, every one found only by
running the thing:

1. `GGML_NATIVE=ON` does not configure alongside `GGML_BACKEND_DL`. That
   branch had never executed anywhere.
2. No `CMAKE_CUDA_ARCHITECTURES`, so Blackwell would have built cleanly
   and failed at runtime.
3. `publish-sidecar.sh` kept an allowlist of two platforms while
   `vendor-sidecar.sh` grew a third — so ten minutes of kernels compiled
   and were refused on the last line.
4. The letter pack planted four strata it never declared, making it
   unrunnable, **while the whole test suite passed** —
   `validate_declared_strata` had only ever been run against synthetic
   fixtures, never a committed bed.

The first three share a cause: a platform was added to the script that
builds and not to the one that publishes, though the first calls the
second directly. A grep for the sibling platform names would have found
all of it in seconds.

The fourth has a better lesson, and it is the one to keep: **a check
that only ever runs against synthetic input reads as coverage and is
not**. It now runs over every committed pack in `bed_sizing.rs`, fails
in 0.17 seconds, and prints the same sentence the pod took an afternoon
to reach.

## What the second run cost

13 August 2026, on a fresh 50GB pod. The provisioning above worked; a
different class of failure took the afternoon, and every one of them
looked like something it was not.

**Disk exhausted three times, and never said so.** A rented volume
enforces a quota, and `df` cannot see it — on a network mount it reports
the whole cluster, so a pod with nothing left prints `452T Avail` and
81% used. The failures then arrive wearing other faces:

- `git` refusing to write `index.lock`, which reads as a permissions or
  lock problem;
- `hf download` ending in `RuntimeError: ... Disk quota exceeded` from
  deep inside a traceback;
- and finally a build that simply **stopped**, mid-link, on the largest
  crate, with no error line anywhere. A killed writer prints nothing. A
  compile that errors prints an error, so *silence at the link step is
  the signature of the process being killed*, not of a slow link.

`pod-eval.sh` now writes a 2GB probe before it spends anything, because
the only reliable reading of a quota is a write.

**Diagnosing it needs `pgrep`, not `top`.** `top -b -n 1` computes
`%CPU` from a single sample and reports `0.0` for everything, so it says
"idle" for a healthy build. `pgrep -af "cargo|rustc|ld"` answers the
only question that matters: is it alive. Note this inverts the eval-time
rule above — during a *compile*, idle CPU means dead; during an *eval*,
idle CPU is normal, because the work is on the GPU.

**A killed build leaves its staging behind.** `vendor-sidecar.sh` cleans
up with `trap ... EXIT`, and SIGKILL runs no traps — so the several GB
of CUDA intermediates that filled the disk are still there for the
retry, which fails the same way. `TMPDIR=/workspace/tmp` (which this
file recommends, because the container overlay is smaller) puts that
tree on the quota'd volume, so it is exactly the wrong thing to leave.
The script now clears stale staging before it starts.

**`du -sh /workspace/*` misses the culprit.** The glob does not match
hidden directories, and `HF_HOME`, `.cache` and staging are all hidden.
Use `du -sh /workspace/* /workspace/.[!.]* 2>/dev/null | sort -h | tail`.

**A multi-line command pasted without backslashes runs as three
commands.** This is how the disk filled in the first place:

```sh
hf download bartowski/Qwen_Qwen3.5-4B-GGUF      # ← ran alone
  Qwen_Qwen3.5-4B-Q4_K_M.gguf                   # ← "command not found"
  --local-dir models/                           # ← "command not found"
```

`hf download <repo>` with no filename is a request for the **entire
repository** — every quantisation, Q2 through Q8, tens of gigabytes. The
tell is in the traceback: `snapshot_download` rather than the
single-file path. The two "command not found" lines scroll past above
the real error and are easy to read as noise.

**The script refused its own output.** `pod-eval.sh` writes
`pod-eval.log`, `pod-baseline-*.json`, `MANIFEST-pod.md` and
`pod-run.tgz` into the repo, none of which were gitignored — so its
dirty-tree check, which exists so a measurement names a commit, would
have refused the *second* run on the first run's results. The tempting
fix at 5pm is `rm` on a run that has not been archived. They are
gitignored now.

**Set `CARGO_PROFILE_DEV_DEBUG=0`.** Debug info is most of `target/`'s
size and all of the link step's peak memory, and nobody attaches a
debugger to a pod. The script exports it.

The lesson to keep is the shape they share: **on a rented box, the
instrument lies about resources, and every failure is reported by
something other than the thing that failed.** Prefer a check that
*does* the thing — write a probe, list the process — over one that
reports on it.

## What the third run cost

14 August 2026, provisioning the v14 exit runs. The afternoon-long
failures above did not recur — the disk probe, the staging cleanup and
the one-line download all held. What remained was cheaper and had one
shape: **the script paid ten minutes to discover something it could
have known in one second.** Twice.

1. **No Rust on the pod.** Vendoring succeeded, because vendoring needs
   cmake and a C++ compiler; the run then died at
   `→ building the runner` on `cargo: command not found`. The
   provisioning notes had a rustup line and it had not been run, and
   nothing between the two noticed.
2. **A pack id that does not exist.** `letter-triage` for
   `app.kttl.letter-to-actions`. The runner refused it correctly and in
   three seconds — after a second CUDA build. Pack ids are fully
   qualified; `kettle packs list` prints them, and it costs nothing to
   run before the script does.

Both are the same missing thing: a preflight. `pod-eval.sh` validates
the weights path and the disk before it spends anything, then validates
the toolchain and the pack id by *using* them, on the far side of the
most expensive step it takes. So:

```sh
cargo --version && cmake --version | head -1 && ls -lh models/*.gguf
cargo run -q -p kettle -- packs list        # after the build, but before a run
```

**Then it happened a third time, after the note was written.** The
renewal run, launched by hand in a second shell, died instantly with
`Exit 127`: the shell predated the `~/.bashrc` edit, and a
non-interactive one would not have read it regardless. Writing the fix
into a runbook did not make it hold, because the thing that needs to
know is the script, not the person. A **by-hand run inherits nothing** —
and the export that fails silently is `CARGO_PROFILE_DEV_DEBUG=0`, whose
absence rebuilds the tree with debug info at the peak disk and peak link
memory that killed 13 August. `Exit 127` is loud; that one is not.

**And a fourth time, 14 August, in a shape the preflight names wrongly.**
The renewal run refused at the preflight with `no cargo on PATH` — which
was true, and the remedy it prints (`. "$CARGO_HOME/env"`) failed with
`bash: /env: No such file or directory`, because `CARGO_HOME` was unset
in that shell so the path collapsed to `/env`. rustup *was* installed.
What was missing was a default toolchain:

```
error: rustup could not choose a version of cargo to run, because one
wasn't specified explicitly, and no default is configured.
```

So the honest sequence on a fresh pod shell is:

```sh
for f in /root/.cargo/env /workspace/.cargo/env "$HOME/.cargo/env"; do
  [ -f "$f" ] && . "$f" && break
done
rustup default stable       # the step a fresh RUSTUP_HOME leaves undone
cargo --version             # prove it before nohup, which starts a new shell
```

`rustup default stable` is the line to remember: pointing `RUSTUP_HOME`
at the data volume, which the note below rightly recommends, gives you a
rustup with no default in it, and the preflight's `cargo: command not
found` sends a reader looking for an install that is already there. The
message is accurate and the diagnosis it suggests is wrong, which is
worse than either alone. No CUDA time was lost — the preflight refused
before vendoring, which is #507 doing its job.

Two smaller things worth keeping:

- **Put `CARGO_HOME` and `RUSTUP_HOME` on the data volume.** The default
  `~/.cargo` is the container overlay, which is the filesystem this file
  already warns is smaller than it looks. `--profile minimal` skips
  clippy and rustfmt, which nothing on a pod runs.
- **`vendor-sidecar.sh` still has no skip-if-present check**, so each of
  these cheap failures cost a full CUDA rebuild — and the dirty-tree
  refusal means editing the script to skip it is not available, by
  design. That is the right trade for provenance and the wrong one for
  iteration; a preflight is how you stop paying it.
