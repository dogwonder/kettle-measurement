#!/usr/bin/env bash
#
# Provision a rented Linux GPU box and run one pack's eval on it.
#
#   ./scripts/pod-eval.sh <pack-id> <weights.gguf>
#
# Run it from a clean checkout on the pod. It vendors llama-server at
# the pinned tag, builds the runner, runs the eval, and leaves a
# tarball you copy back.
#
# ## What this is for
#
# A full letter run is ~2h of local GPU. Renting one buys that time
# back. Nothing else about the measurement changes: same commit, same
# weights, same pinned llama.cpp tag, same fixtures.
#
# ## The privacy line, which is not negotiable
#
# **Bed fixtures only.** Every fixture in `packs/*/fixtures/` is wholly
# synthetic and may be copied to a rented machine. Nothing derived from
# a real document ever may: no `*.private.*` input, no OCR or transcript
# of one, no field-evidence run. Those live inside Kettle's deletion
# boundary, and a box somebody else owns is exactly where that boundary
# gets crossed by accident. This script therefore takes a pack id and
# never a `--fixture-dir`, so there is no argument through which a
# private path could reach the pod.
#
# ## What a pod run can and cannot tell you
#
# The score is the model's; the *timing* is the pod's, and a tier claim
# is a sentence about somebody's own laptop. Never merge a pod timing
# into `tiers.json` as a user-facing tier.
#
# Whether scores reproduce across backends **has now been measured
# once**, on 24 August 2026: the letter development bed at scoring 15,
# same commit, same bed digest, same weights, M1 Pro (Metal) against a
# rented RTX 3090 (CUDA). All 56 comparable extraction strata reported
# identical precision on identical denominators. So a pod baseline and a
# Mac baseline are no longer presumed to be two instruments — on that
# bed, at that scoring version, they agreed exactly.
#
# It is one pack on one bed and not a general law. A prompt edit judged
# solely across machines is still worth a second thought, and #320
# refuses the comparison outright whenever the bed moved. The full
# argument and the numbers are in `evals/RENTED-GPU.md`; this header
# used to say the equivalence was asserted and never measured, which was
# true until that run and is the kind of drift a playbook read at 2am
# will not catch on its own.

set -euo pipefail

PACK="${1:?usage: pod-eval.sh <pack-id> <weights.gguf> [eval flags...]}"
WEIGHTS="${2:?usage: pod-eval.sh <pack-id> <weights.gguf> [eval flags...]}"
shift 2
# Anything further is passed to `kettle eval` as given. This exists for
# `--runs`, which is the one flag a rented box is *for*: a stability
# check is three bed runs, ~35 minutes here against five and a half
# hours on an M1 Pro, which is why #533's claims stood on `--runs 1`
# for as long as they did. Deliberately a pass-through and not a named
# option — the refusals above are about the machine, and the eval's own
# flags are the eval's business. `--fixture-dir` stays impossible
# because it is never *offered*, and adding it here would need typing
# out a private path in full.
EVAL_FLAGS=("$@")

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo"

if [[ ! -f "$WEIGHTS" ]]; then
  echo "no weights at $WEIGHTS" >&2
  echo "copy the .gguf to the pod first — it is gitignored and ships with nothing" >&2
  exit 1
fi

# ## Everything knowable in one second, before the ten-minute build
#
# On 14 August this script twice discovered a prerequisite at line 109,
# after vendoring: once a missing Rust toolchain (`cargo: command not
# found`), once a pack id that does not exist (`letter-triage` for
# `app.kttl.letter-to-actions`). Both refusals were correct and both
# arrived on the far side of a CUDA build, so each cost ten minutes to
# say something knowable at the start. A rented box makes that a bill.
# The message matters as much as the refusal. On 14 August this printed
# "no cargo on PATH", which was true, and sent a reader looking for an
# install that was already there: rustup was present with no default
# toolchain, and the suggested `. "$CARGO_HOME/env"` expanded to `/env`
# because the variable was unset in that shell. An accurate message with
# a wrong diagnosis is worse than either alone, so this one asks which
# of the three states it is in and says so.
if ! command -v cargo >/dev/null; then
  echo "no cargo on PATH" >&2
  if command -v rustup >/dev/null; then
    echo "rustup is installed, so this is a PATH problem, not a missing toolchain:" >&2
    echo "  . \"\$(dirname \"\$(command -v rustup)\")/env\"" >&2
  else
    for env_file in /root/.cargo/env /workspace/.cargo/env "$HOME/.cargo/env"; do
      if [[ -f "$env_file" ]]; then
        echo "a toolchain is installed but this shell has not sourced it:" >&2
        echo "  . \"$env_file\"" >&2
        found_env=1
        break
      fi
    done
    if [[ -z "${found_env:-}" ]]; then
      echo "and no toolchain is installed. Put it on the data volume, not the" >&2
      echo "container overlay, so the next shell still has it:" >&2
      echo "  export CARGO_HOME=/workspace/.cargo RUSTUP_HOME=/workspace/.rustup" >&2
      echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y" >&2
    fi
  fi
  echo "see evals/RENTED-GPU.md — this is the failure that costs a CUDA rebuild" >&2
  exit 1
fi

# rustup with no default toolchain is a fourth state, and it reads as a
# working install until the moment it does not (14 August): `cargo` is on
# PATH as a rustup shim, and running it fails with "could not choose a
# version of cargo to run". Asked here, where it costs a second.
if ! cargo --version >/dev/null 2>&1; then
  echo "cargo is on PATH but cannot run" >&2
  echo "a fresh RUSTUP_HOME has no default toolchain in it:" >&2
  echo "  rustup default stable" >&2
  echo "see evals/RENTED-GPU.md" >&2
  exit 1
fi

# `--resume` is refused alongside `--runs`, and correctly: a repeat that
# reused another repeat's answers would measure nothing and report
# perfect stability, which is the one wrong answer a stability check
# must not be able to give. Decided here rather than at the call, with
# the rest of the knowable-in-one-second checks: a person who asked for
# three runs should learn what this script did with that before the
# CUDA build, not after it. The cost is worth stating too — a stability
# run has no crash insurance, so run it under tmux and expect to start
# over if it dies.
resume=(--resume)
for flag in "${EVAL_FLAGS[@]}"; do
  if [[ "$flag" == "--runs" || "$flag" == --runs=* ]]; then
    resume=()
    echo "→ --runs given, so not resuming: repeats must not reuse each other"
    break
  fi
done

if [[ ! -f "packs/$PACK/pack.json" ]]; then
  echo "no pack called $PACK in packs/" >&2
  echo "pack ids are fully qualified; the repository ships:" >&2
  for manifest in packs/*/pack.json; do
    echo "  $(basename "$(dirname "$manifest")")" >&2
  done
  exit 1
fi

# ## Disk, before anything expensive
#
# A rented volume enforces a **quota**, and `df` cannot see it: on a
# network mount it reports the whole cluster, so a pod with 50GB and
# nothing left prints hundreds of terabytes available. The 13 August run
# died three times on this — the last time silently, mid-link, with no
# error in the log at all, because the writer was killed rather than
# refused.
#
# So the check is a write, not a reading. A probe large enough to be
# meaningful and small enough to be quick.
: "${TMPDIR:=/workspace/tmp}"
export TMPDIR
mkdir -p "$TMPDIR"

# A build killed by the quota leaves its staging tree behind:
# `vendor-sidecar.sh` cleans up with `trap ... EXIT`, and a SIGKILL runs
# no traps. So the leftovers of the failure keep the disk full for the
# retry, which then fails the same way.
stale="$(find "$TMPDIR" -maxdepth 1 -name 'tmp.*' -type d 2>/dev/null | head -20)"
if [[ -n "$stale" ]]; then
  echo "→ clearing staging left by an interrupted build"
  echo "$stale" | while read -r dir; do rm -rf "$dir"; done
fi

echo "→ checking there is room to work"
probe="$TMPDIR/.pod-eval-probe"
if ! dd if=/dev/zero of="$probe" bs=1M count=2048 status=none 2>/dev/null; then
  rm -f "$probe"
  echo "cannot write 2GB to $TMPDIR — the volume is full or over quota" >&2
  echo "the CUDA build alone needs several GB, so this would die mid-compile" >&2
  echo "  du -sh /workspace/* /workspace/.[!.]* 2>/dev/null | sort -h | tail" >&2
  exit 1
fi
rm -f "$probe"

# Debug info is most of `target/`'s size and all of the link step's peak
# memory, and nobody attaches a debugger to a pod. Turning it off is
# several GB that the run directories can have instead.
export CARGO_PROFILE_DEV_DEBUG=0

# A run's provenance is only worth as much as the tree it names, and a
# dirty tree names nothing reproducible.
if [[ -n "$(git status --porcelain)" ]]; then
  echo "the working tree is dirty; a measurement should name a commit" >&2
  git status --short >&2
  exit 1
fi
commit="$(git rev-parse HEAD)"

echo "→ vendoring llama-server at the pinned tag"
./scripts/vendor-sidecar.sh

echo "→ building the runner"
cargo build --locked -p kettle

echo "→ checking the tree agrees with itself before spending GPU hours"
cargo test --locked -q

echo "→ running the eval: $PACK"
# No --baseline: the pod is a different instrument from whatever
# recorded the committed baselines, and a bed change refuses the
# comparison anyway. The baseline is written, read on the other side,
# and adopted deliberately or not at all.
out="pod-baseline-$(basename "$PACK").json"
# `--resume` reuses fixtures already scored under an identical key —
# same model, pack version, bed digest and scoring version — so a run
# killed at fixture 700 of 794 picks up rather than starting over. It is
# a no-op on a first run, and the moment a person reaches for this
# script a second time is exactly the moment they want it. The 11 August
# letter run lost 326 fixtures to a dropped connection without it.
cargo run --locked -p kettle -- eval "$PACK" \
  --model "$WEIGHTS" \
  "${resume[@]}" \
  --write-baseline "$out" \
  "${EVAL_FLAGS[@]}" \
  2>&1 | tee "pod-eval.log"

cat > MANIFEST-pod.md <<EOF
# Pod run — $PACK

- **Commit**: $commit
- **Weights**: $(basename "$WEIGHTS")
- **Eval flags**: ${EVAL_FLAGS[*]:-none}
- **Host**: $(uname -sm), $(nproc) cores
- **GPU**: $(command -v nvidia-smi >/dev/null && nvidia-smi --query-gpu=name --format=csv,noheader | head -1 || echo "none — CPU build")
- **llama.cpp**: pinned tag, built by scripts/vendor-sidecar.sh on this host
- **Run on rented hardware**: yes. Bed fixtures only; no private input
  reached this machine.

Timings here are the pod's and are not a tier claim about anyone's
laptop. See scripts/pod-eval.sh for what a cross-instrument measurement
does and does not license.
EOF

# Every run directory, not `run1`. `--runs 3` writes run1..run3 and this
# line named the first, so a stability run came home with its scores —
# aggregated into the baseline — and without two thirds of the answers
# they were computed from. Nothing looked wrong, which is the bad kind
# of bug: the recordings exist so a score can be re-asked under new
# scoring without re-running the GPU, and "did the repeats agree?" is
# exactly the question a missing repeat can never answer again.
tar czf pod-run.tgz evals/runs/run* "$out" pod-eval.log MANIFEST-pod.md
echo
echo "done — copy pod-run.tgz back, e.g.:"
echo "  scp <pod>:$repo/pod-run.tgz ."
