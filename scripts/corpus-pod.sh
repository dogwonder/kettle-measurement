#!/usr/bin/env bash
#
# The bounded old/new letter-prompt comparison on a rented Linux CUDA box
# (evals/corpus/measurement-02-pod.md). This is NOT `pod-eval.sh`: that
# script runs a whole pack bed and writes a baseline; this one runs
# `kettle corpus` on two frozen selections under each of two frozen
# packs, on one pod, under one deadline per arm, and never writes a
# baseline or a tier.
#
#   local   corpus-pod.sh bundle [out.tgz]      tracked files only, hashed
#   local   corpus-pod.sh check-bundle <tgz>    extract, build, no-model checks
#   pod     corpus-pod.sh preflight             one-second checks, no spending
#   pod     corpus-pod.sh setup                 CUDA sidecar, weights, CLI, no-model checks
#   pod     corpus-pod.sh freeze                hash everything → measurement-02-pod.frozen.json
#   pod     corpus-pod.sh run old|new           one arm, both selections, one 1200 s deadline
#   pod     corpus-pod.sh replay old|new        exact replay with the arm's own pack, compared
#   pod     corpus-pod.sh pack                  tarball for the trip home
#   either  corpus-pod.sh exchange-summary <arm-dir>   recount exchanges by kind
#   either  corpus-pod.sh replay-check <arm-dir>       recompare <sel> with <sel>-replay
#
# Every pod step runs from the extracted bundle root. `run` refuses
# without a frozen plan whose pins still match, refuses an output
# directory that exists, refuses a second arm after an incomplete first
# one, and never reruns anything. What it enforces is what the plan
# says; what it cannot enforce (rental, approval) it prints.
#
# The privacy line is inherited from `bundle`: `git archive` of tracked
# paths, so no `*.private.*` file can travel, and the bundle refuses any
# path that looks like one anyway.

set -euo pipefail

PLAN_REL="evals/corpus/measurement-02-pod.json"
FROZEN_REL="evals/corpus/measurement-02-pod.frozen.json"
WEIGHTS_REL="models/qwen3.5-4b-q4_k_m.gguf"
SIDECAR_DIR_REL="sidecars/linux-x86_64"
KETTLE_REL="target/debug/kettle"
DEADLINE_SECONDS=1200
RUST_TOOLCHAIN="1.97.1"

root="$(pwd)"
cmd="${1:-}"
[[ -n "$cmd" ]] || { sed -n 2,26p "$0" | sed 's/^# \{0,1\}//' >&2; exit 2; }
shift

die() { echo "corpus-pod: $*" >&2; exit 1; }
note() { echo "→ $*"; }

sha256_of() {
  if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1
  else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

# Monotonic seconds. /proc/uptime on Linux; the local check-bundle path
# never needs a deadline, so macOS falls back to the wall clock.
mono() {
  if [[ -r /proc/uptime ]]; then cut -d. -f1 /proc/uptime; else date +%s; fi
}

plan_get() { # plan_get <file> <python expression over d>
  python3 -c "import json,sys; d=json.load(open(sys.argv[1])); print(eval(sys.argv[2]))" "$1" "$2"
}

# The fixed environment every CLI launch runs under (both arms, both
# replays). RuntimeIdentity hashes the whole environment, so two shells
# with different SHLVL or OLDPWD would give the two arms two runtime
# identities for no reason a reader could act on.
fixed_env() {
  echo "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin" "HOME=$HOME" "LANG=C.UTF-8"
}

# ---------------------------------------------------------------- bundle
# Local. Tracked files only, from HEAD, plus the old pack from the base
# commit the plan names. The 82 runtime source files are exactly the
# ones the Metal freeze pinned, so `freeze` can prove the pod built the
# same source. No fixtures, no tests, no weights, no sidecar.
bundle() {
  local out="${1:-pod-corpus-bundle-$(git rev-parse --short HEAD).tgz}"
  [[ -f "$PLAN_REL" ]] || die "no $PLAN_REL"
  local head base
  head="$(git rev-parse HEAD)"
  base="$(plan_get "$PLAN_REL" "d['implementation']['base_commit']")"
  # Everything comes out of `git archive HEAD`, so an untracked file or a
  # modification elsewhere in the checkout cannot travel. What would
  # mislead a reader is a modified copy of a path the bundle takes, since
  # the bundle would then name a commit the author was not looking at.
  # Refuse exactly that; another session's work elsewhere is only noted.
  local bundled
  bundled="$(plan_get "$PLAN_REL" "'\n'.join(sorted(set(list(d['implementation']['source_files']) + list(d['inventory_files']) + [c['path'] for c in d['corpora']] + [f for a in d['arms'] for f in a['pack_files']] + ['Cargo.toml','Cargo.lock','scripts/corpus-pod.sh','scripts/vendor-sidecar.sh','scripts/publish-sidecar.sh','evals/corpus/measurement-02.json','evals/corpus/measurement-02.md','evals/corpus/measurement-02-pod.json','evals/corpus/measurement-02-pod.md','evals/RENTED-GPU.md','sidecars/README.md'])))")"
  local dirty_bundled
  dirty_bundled="$(echo "$bundled" | xargs git status --porcelain -- 2>/dev/null || true)"
  [[ -z "$dirty_bundled" ]] || { echo "$dirty_bundled" >&2; die "a path the bundle takes is modified; commit it, since the bundle names $head"; }
  if [[ -n "$(git status --porcelain)" ]]; then
    note "other changes in the checkout are not part of this bundle:"; git status --short | sed 's/^/    /'
  fi

  local stage
  stage="$(mktemp -d)"
  # Expanded now: the variable is local and the trap fires after the function returns.
  trap "rm -rf '$stage'" EXIT

  note "runtime sources pinned by the plan, from $head"
  mapfile -t sources < <(plan_get "$PLAN_REL" "'\n'.join(sorted(d['implementation']['source_files']))")
  git archive "$head" "${sources[@]}" | tar -x -C "$stage"

  note "corpora, inventory, plans, scripts"
  local extras=(
    evals/corpus/diagnostic-01.json evals/corpus/product-regressions-01.json
    evals/corpus/measurement-02.json evals/corpus/measurement-02.md
    evals/corpus/measurement-02-pod.json evals/corpus/measurement-02-pod.md
    evals/RENTED-GPU.md sidecars/README.md
    scripts/vendor-sidecar.sh scripts/publish-sidecar.sh scripts/corpus-pod.sh
  )
  mapfile -t inventory < <(plan_get "$PLAN_REL" "'\n'.join(sorted(d['inventory_files']))")
  git archive "$head" "${extras[@]}" "${inventory[@]}" | tar -x -C "$stage"

  note "one pack per arm, each from the commit its plan entry names"
  local arm src
  for arm in $(plan_get "$PLAN_REL" "' '.join(a['id'] for a in d['arms'])"); do
    src="$(plan_get "$PLAN_REL" "next(a for a in d['arms'] if a['id']=='$arm')['pack_source']")"
    [[ "$src" == HEAD ]] && src="$head"
    mapfile -t arm_files < <(plan_get "$PLAN_REL" "'\n'.join(sorted(next(a for a in d['arms'] if a['id']=='$arm')['pack_files']))")
    mkdir -p "$stage/inputs/$arm" "$stage/.tmp-$arm"
    git archive "$src" "${arm_files[@]}" | tar -x -C "$stage/.tmp-$arm"
    mv "$stage/.tmp-$arm/packs/app.kttl.letter-to-actions" "$stage/inputs/$arm/pack"
    rm -rf "$stage/.tmp-$arm"
    echo "  $arm ← $src"
  done

  note "refusing anything that should not travel"
  local bad
  bad="$(cd "$stage" && find . -type f \( -name '*.private.*' -o -name '*.gguf' -o -path '*/fixtures/*' -o -path '*/tests/*' \) | head)"
  [[ -z "$bad" ]] || { echo "$bad" >&2; die "bundle contains a path that must not travel"; }

  note "manifest"
  (cd "$stage" && python3 - "$head" "$base" <<'PY'
import hashlib, json, os, sys
files = {}
for d, _, names in os.walk('.'):
    for n in names:
        p = os.path.normpath(os.path.join(d, n))
        files[p] = 'sha256:' + hashlib.sha256(open(p, 'rb').read()).hexdigest()
plan = json.load(open('evals/corpus/measurement-02-pod.json'))
m = {'schema': 'kettle/corpus-pod-bundle@1', 'head_commit': sys.argv[1], 'base_commit': sys.argv[2],
     'plan_id': plan['id'], 'files': dict(sorted(files.items()))}
# Check the bundle against the plan's own pins before it leaves.
bad = []
for rel, want in plan['implementation']['source_files'].items():
    if files.get(rel) != want: bad.append(rel)
for sub in plan['arms']:
    for rel, want in sub['pack_files'].items():
        local = 'inputs/%s/pack/%s' % (sub['id'], rel.split('packs/app.kttl.letter-to-actions/', 1)[1])
        if files.get(local) != want: bad.append(local)
for c in plan['corpora']:
    if files.get(c['path']) != c['digest']: bad.append(c['path'])
for rel, want in plan['inventory_files'].items():
    if files.get(rel) != want: bad.append(rel)
if bad:
    print('bundle disagrees with the plan pins:\n  ' + '\n  '.join(bad), file=sys.stderr); sys.exit(1)
json.dump(m, open('BUNDLE-MANIFEST.json', 'w'), indent=2)
print('  %d files, every plan pin matches' % len(files))
PY
  )

  tar czf "$out" -C "$stage" .
  echo "bundle: $out ($(du -h "$out" | cut -f1)) sha256 $(sha256_of "$out")"
  echo "head $head, base $base"
}

# ---------------------------------------------------------- check-bundle
# Local. Extract into a scratch directory, build the CLI exactly as the
# pod will, and run the four no-model acquisition checks the Metal
# freeze ran. Proves the file set is sufficient; a deterministic floor
# supplies no reading result.
check_bundle() {
  local tgz="${1:?check-bundle <bundle.tgz>}"
  local work="${2:-$(mktemp -d)}"
  mkdir -p "$work"
  note "extracting into $work"
  tar xzf "$tgz" -C "$work"
  (cd "$work" && python3 - <<'PY'
import hashlib, json, os, sys
m = json.load(open('BUNDLE-MANIFEST.json'))
bad = [p for p, h in m['files'].items()
       if 'sha256:' + hashlib.sha256(open(p, 'rb').read()).hexdigest() != h]
if bad: print('manifest mismatch: ' + ', '.join(bad), file=sys.stderr); sys.exit(1)
print('  %d files match BUNDLE-MANIFEST.json' % len(m['files']))
PY
  )
  note "building the CLI (--features pdf) in $work"
  (cd "$work" && cargo build --locked -p kettle --features pdf 2>&1 | tail -3)
  no_model_checks "$work"
  echo "check-bundle: ok in $work"
}

# Four `--no-model` runs: each pack over each selection. Exit 0 and zero
# unscored cases is the bar, exactly as the Metal freeze recorded.
no_model_checks() {
  local base="${1:-$root}"
  local kettle="$base/$KETTLE_REL"
  [[ -x "$kettle" ]] || die "no CLI at $kettle"
  local outroot="$base/setup/no-model-$(date -u +%Y%m%dT%H%M%SZ)"
  mkdir -p "$outroot"
  local arm sel corpus
  for arm in $(plan_get "$base/$PLAN_REL" "' '.join(a['id'] for a in d['arms'])"); do
    for sel in diagnostic-01 product-regressions-01; do
      corpus="$base/evals/corpus/$sel.json"
      note "no-model $arm/$sel"
      (cd "$base" && "$kettle" corpus --no-model \
        --corpus "$corpus" --inventory-dir "$base/evals/capabilities" \
        --pack-dir "$base/inputs/$arm/pack" \
        --out "$outroot/$arm-$sel" >"$outroot/$arm-$sel.stdout" 2>"$outroot/$arm-$sel.stderr") \
        || { tail -5 "$outroot/$arm-$sel.stderr" >&2; die "no-model run failed for $arm/$sel"; }
      python3 - "$outroot/$arm-$sel/report.json" <<'PY'
import json, sys
r = json.load(open(sys.argv[1]))
n = len(r['cases']); u = r.get('unscored_cases', 0)
assert r.get('model') in (None, {}), 'a no-model run recorded a model identity'
print('  %d cases, %d unscored, answer_source=%s' % (n, u, r.get('answer_source')))
sys.exit(0 if u == 0 else 1)
PY
    done
  done
  echo "no-model checks: every arm over both selections passed under $outroot"
}

# ------------------------------------------------------------- preflight
# Pod. Everything knowable in a second. Writes only two probe files.
preflight() {
  echo "== corpus-pod preflight at $(date -u +%FT%TZ) on $(hostname)"
  [[ -f "$PLAN_REL" && -f BUNDLE-MANIFEST.json ]] || die "run from the extracted bundle root"
  echo "-- GPU"
  command -v nvidia-smi >/dev/null || die "no nvidia-smi: not a GPU box"
  nvidia-smi --query-gpu=name,driver_version,memory.total,memory.used,utilization.gpu --format=csv
  local gpu; gpu="$(nvidia-smi --query-gpu=name --format=csv,noheader | head -1)"
  local want; want="$(plan_get "$PLAN_REL" "d['pod']['gpu_expected']")"
  [[ "$gpu" == "$want" ]] || echo "WARNING: plan expects '$want', box has '$gpu' — set KETTLE_CUDA_ARCH for it and say so in the frozen plan"
  if pgrep -f llama-server >/dev/null; then die "a llama-server is already running on this box"; fi
  echo "-- CUDA toolkit"
  local cuda="${CUDA_PATH:-/usr/local/cuda}"
  [[ -x "$cuda/bin/nvcc" ]] && "$cuda/bin/nvcc" --version | tail -2 || echo "WARNING: no nvcc at $cuda/bin — set CUDA_PATH to the toolkit (ls /usr/local/cuda*)"
  echo "-- tools"
  local t; for t in cmake cc c++ curl tar python3 git; do
    printf '  %-8s %s\n' "$t" "$(command -v "$t" || echo MISSING)"
  done
  if command -v cargo >/dev/null && cargo --version >/dev/null 2>&1; then cargo --version; else echo "  cargo    not usable yet (setup installs rustup $RUST_TOOLCHAIN if missing)"; fi
  echo "-- filesystems (a write, not df: df lies on a network mount)"
  local d; for d in "$root" /workspace; do
    [[ -d "$d" ]] || continue
    local probe="$d/.corpus-pod-probe"
    local t0; t0=$(date +%s.%N)
    if dd if=/dev/zero of="$probe" bs=1M count=512 conv=fsync status=none 2>/dev/null; then
      printf '  %-12s 512MB in %ss  (%s)\n' "$d" "$(python3 -c "import time;print(round(time.time()-$t0,2))")" "$(df -h "$d" | tail -1 | awk '{print $2" total, "$4" avail (df, unreliable on a network mount)"}')"
    else echo "  $d: cannot write 512MB"; fi
    rm -f "$probe"
  done
  echo "-- memory"; free -g | head -2
  echo "-- cost: record costPerHr at freeze (runpodctl pod get <id> from home); the pod is billing now"
  echo "preflight: done. Nothing was built or downloaded."
}

# ------------------------------------------------------------------ setup
# Pod. The expensive steps, in the order that fails cheapest first.
setup() {
  [[ -f "$PLAN_REL" && -f BUNDLE-MANIFEST.json ]] || die "run from the extracted bundle root"
  mkdir -p setup
  local log="setup/setup-$(date -u +%Y%m%dT%H%M%SZ).log"
  exec > >(tee -a "$log") 2>&1
  echo "== corpus-pod setup at $(date -u +%FT%TZ)"

  # CUDA: all four exports, or the build silently omits the backend
  # (evals/RENTED-GPU.md, 1 September 2026). The arch is the card's.
  export CUDA_PATH="${CUDA_PATH:-/usr/local/cuda}"
  [[ -x "$CUDA_PATH/bin/nvcc" ]] || die "no nvcc at $CUDA_PATH/bin; export CUDA_PATH first"
  export CUDACXX="$CUDA_PATH/bin/nvcc" CUDAToolkit_ROOT="$CUDA_PATH" PATH="$CUDA_PATH/bin:$PATH"
  local gpu; gpu="$(nvidia-smi --query-gpu=name --format=csv,noheader | head -1)"
  if [[ -z "${KETTLE_CUDA_ARCH:-}" ]]; then
    case "$gpu" in
      *"RTX 4090"*) export KETTLE_CUDA_ARCH=89 ;;
      *"RTX 3090"*) export KETTLE_CUDA_ARCH=86 ;;
      *"RTX 5090"*) export KETTLE_CUDA_ARCH=120 ;;
      *) die "unknown card '$gpu': export KETTLE_CUDA_ARCH yourself (sm_XY without the dot)" ;;
    esac
  fi
  echo "GPU $gpu, KETTLE_CUDA_ARCH=$KETTLE_CUDA_ARCH, CUDA_PATH=$CUDA_PATH"

  export TMPDIR="$root/tmp"; mkdir -p "$TMPDIR"
  export CARGO_PROFILE_DEV_DEBUG=0
  note "2GB write probe in $TMPDIR"
  dd if=/dev/zero of="$TMPDIR/.probe" bs=1M count=2048 status=none || die "cannot write 2GB to $TMPDIR"
  rm -f "$TMPDIR/.probe"

  # Rust: the toolchain the Metal freeze was built with. `rustup default`
  # is what a fresh RUSTUP_HOME leaves undone (RENTED-GPU.md, 14 August).
  if ! command -v rustup >/dev/null; then
    for f in /root/.cargo/env /workspace/.cargo/env "$HOME/.cargo/env"; do [[ -f "$f" ]] && . "$f" && break; done
  fi
  if ! command -v rustup >/dev/null; then
    note "installing rustup (minimal) into ${CARGO_HOME:-$HOME/.cargo}"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain "$RUST_TOOLCHAIN"
    . "${CARGO_HOME:-$HOME/.cargo}/env"
  fi
  rustup toolchain install "$RUST_TOOLCHAIN" --profile minimal >/dev/null
  rustup default "$RUST_TOOLCHAIN" >/dev/null
  cargo --version; rustc --version

  # Sidecar at the pinned tag. The artefact is the test, not the flag.
  if compgen -G "$SIDECAR_DIR_REL/libggml-cuda.so*" >/dev/null && [[ -x "$SIDECAR_DIR_REL/llama-server" ]]; then
    note "sidecar already vendored with a CUDA backend; not rebuilding"
  else
    note "vendoring llama-server b10145 with CUDA (this is the ten-minute step)"
    ./scripts/vendor-sidecar.sh
  fi
  compgen -G "$SIDECAR_DIR_REL/libggml-cuda.so*" >/dev/null || die "no libggml-cuda.so in $SIDECAR_DIR_REL: CPU-only build, stop"
  "$SIDECAR_DIR_REL/llama-server" --version 2>&1 | head -2

  # Weights: every model the plan names (the default and any per-arm
  # override), downloaded once and verified against the pinned SHA-256
  # and byte count. One line each, never split.
  mkdir -p models
  local spec
  while IFS='|' read -r mpath msha mbytes mrepo mfile; do
    ensure_weights "$mpath" "$msha" "$mbytes" "$mrepo" "$mfile"
  done < <(plan_get "$PLAN_REL" "'\n'.join('|'.join([m['path'], m['digest'].split(':')[1], str(m['bytes']), m['hf_repo'], m['hf_file']]) for m in [d['model']] + [a['model'] for a in d['arms'] if a.get('model')])" | sort -u)

  # `--features pdf`, and not the default set: since 70c2a1a8 the runner
  # calls chrono::Utc::now(), which only compiles when pdfium-render pulls
  # chrono's clock feature in. A default-feature build fails on main; an
  # --all-features build would drag the macOS-only Vision crates onto
  # Linux. No PDF is read here, so the feature is a compile-time fact only.
  note "building the CLI (--features pdf)"
  cargo build --locked -p kettle --features pdf
  "$KETTLE_REL" corpus --help >/dev/null

  no_model_checks "$root"
  echo "setup: done. Next: corpus-pod.sh freeze, then send the frozen plan home for review. No model has been started."
}

ensure_weights() { # <path> <sha256> <bytes> <hf repo> <hf file>
  local mpath="$1" want_sha="$2" want_bytes="$3" repo="$4" file="$5"
  if [[ ! -f "$mpath" ]]; then
    note "downloading $file (PEP 668 boxes need a venv)"
    [[ -x .venv/bin/hf ]] || { python3 -m venv .venv && .venv/bin/pip install -q -U "huggingface_hub[hf_transfer]"; }
    HF_HUB_ENABLE_HF_TRANSFER=1 .venv/bin/hf download "$repo" "$file" --local-dir models/
    mv "models/$file" "$mpath"
  fi
  note "verifying $mpath"
  local got_sha; got_sha="$(sha256_of "$mpath")"
  local got_bytes; got_bytes="$(stat -c %s "$mpath" 2>/dev/null || stat -f %z "$mpath")"
  [[ "$got_sha" == "$want_sha" && "$got_bytes" == "$want_bytes" ]] || die "weights mismatch for $mpath: got $got_sha ($got_bytes bytes), want $want_sha ($want_bytes)"
  echo "weights ok: $mpath $got_sha"
}

# ----------------------------------------------------------------- freeze
# Pod. Hash the executable, every sidecar file, the weights and every
# input against the plan, record the box, and write the frozen plan.
# Starts no model. The output is what run approval is requested on.
freeze() {
  [[ -f "$PLAN_REL" ]] || die "no $PLAN_REL"
  [[ -x "$KETTLE_REL" ]] || die "no CLI; run setup first"
  compgen -G "$SIDECAR_DIR_REL/libggml-cuda.so*" >/dev/null || die "no CUDA backend in $SIDECAR_DIR_REL"
  [[ -f "$WEIGHTS_REL" ]] || die "no weights at $WEIGHTS_REL"
  local cuda="${CUDA_PATH:-/usr/local/cuda}"
  HOME_FOR_ENV="$HOME" KETTLE_ROOT="$root" python3 - "$PLAN_REL" "$FROZEN_REL" "$KETTLE_REL" "$SIDECAR_DIR_REL" "$WEIGHTS_REL" "$cuda" "${KETTLE_CUDA_ARCH:-89}" <<'PY'
import hashlib, json, os, platform, subprocess, sys, datetime
plan_p, frozen_p, kettle, sdir, weights, cuda, arch = sys.argv[1:8]
def sha(p):
    h = hashlib.sha256()
    with open(p, 'rb') as f:
        for chunk in iter(lambda: f.read(1 << 20), b''): h.update(chunk)
    return 'sha256:' + h.hexdigest()
def run(*a):
    try: return subprocess.run(a, capture_output=True, text=True, timeout=60).stdout.strip()
    except Exception as e: return 'unavailable: %s' % e
plan = json.load(open(plan_p))
bad = []
for rel, want in plan['implementation']['source_files'].items():
    if not os.path.exists(rel) or sha(rel) != want: bad.append(rel)
for sub in plan['arms']:
    for rel, want in sub['pack_files'].items():
        local = 'inputs/%s/pack/%s' % (sub['id'], rel.split('packs/app.kttl.letter-to-actions/', 1)[1])
        if not os.path.exists(local) or sha(local) != want: bad.append(local)
for sub in plan['arms']:
    m = sub.get('model')
    if m and sha(m['path']) != m['digest']: bad.append(m['path'])
for c in plan['corpora']:
    if sha(c['path']) != c['digest']: bad.append(c['path'])
for rel, want in plan['inventory_files'].items():
    if sha(rel) != want: bad.append(rel)
if sha(weights) != plan['model']['digest']: bad.append(weights)
if bad:
    print('freeze refused; these differ from the plan pins:\n  ' + '\n  '.join(bad), file=sys.stderr); sys.exit(1)
f = dict(plan)
f['status'] = 'frozen on the pod; execution not authorised until this file is reviewed at home; no model started'
f['executable'] = dict(plan['executable'], digest=sha(kettle), rustc=run('rustc', '--version'), cargo=run('cargo', '--version'))
files = {n: sha(os.path.join(sdir, n)) for n in sorted(os.listdir(sdir)) if os.path.isfile(os.path.join(sdir, n))}
f['sidecar'] = dict(plan['sidecar'], files=files, version_output=run(os.path.join(sdir, 'llama-server'), '--version') or run('sh', '-c', '%s --version 2>&1' % os.path.join(sdir, 'llama-server')))
nv = run('nvidia-smi', '--query-gpu=name,driver_version,memory.total', '--format=csv,noheader').split(', ')
osr = {}
try:
    for line in open('/etc/os-release'):
        if '=' in line: k, v = line.rstrip().split('=', 1); osr[k] = v.strip('"')
except Exception: pass
cpu = 'unknown'
try:
    for line in open('/proc/cpuinfo'):
        if line.startswith('model name'): cpu = line.split(':', 1)[1].strip(); break
except Exception: pass
ram = None
try:
    for line in open('/proc/meminfo'):
        if line.startswith('MemTotal'): ram = round(int(line.split()[1]) / 1048576); break
except Exception: pass
f['pod'] = dict(plan['pod'], gpu=nv[0] if nv else None, driver=nv[1] if len(nv) > 1 else None,
    gpu_memory=nv[2] if len(nv) > 2 else None,
    cuda_runtime=run('nvidia-smi').split('CUDA Version:')[-1].split('|')[0].strip() if 'CUDA Version' in run('nvidia-smi') else None,
    nvcc=run(os.path.join(cuda, 'bin', 'nvcc'), '--version').splitlines()[-1] if os.path.exists(os.path.join(cuda, 'bin', 'nvcc')) else None,
    cuda_arch=arch, os=osr.get('PRETTY_NAME', platform.platform()), kernel=platform.release(), cpu=cpu,
    nproc=os.cpu_count(), ram_gb=ram, hostname=platform.node(),
    frozen_at=datetime.datetime.now(datetime.timezone.utc).isoformat())
f['environment'] = dict(plan['environment'], HOME=os.environ['HOME_FOR_ENV'])
f['bundle_manifest_digest'] = sha('BUNDLE-MANIFEST.json')
f['prepared_plan_digest'] = sha(plan_p)
json.dump(f, open(frozen_p, 'w'), indent=2); open(frozen_p, 'a').write('\n')
print('frozen: %s' % frozen_p)
print('  executable %s' % f['executable']['digest'])
print('  sidecar    %d files, %s' % (len(files), f['sidecar']['version_output'].splitlines()[0] if f['sidecar']['version_output'] else '?'))
print('  gpu        %s, driver %s, CUDA %s, arch %s' % (f['pod']['gpu'], f['pod']['driver'], f['pod']['cuda_runtime'], arch))
print('  toolchain  %s' % f['executable']['rustc'])
print('  every source, pack, corpus, inventory and weight pin matches the plan')
PY
  echo "freeze: done. Fill pod.cost_per_hour by hand from runpodctl, send $FROZEN_REL home, and wait for run approval."
}

# Re-verify what `freeze` pinned, right before spending. Refuses on any
# difference; nothing is ever silently replaced.
verify_frozen() {
  [[ -f "$FROZEN_REL" ]] || die "no $FROZEN_REL: run freeze first, then get it approved"
  python3 - "$FROZEN_REL" "$KETTLE_REL" "$SIDECAR_DIR_REL" "$WEIGHTS_REL" <<'PY'
import hashlib, json, os, sys
frozen_p, kettle, sdir, weights = sys.argv[1:5]
def sha(p):
    h = hashlib.sha256()
    with open(p, 'rb') as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b''): h.update(chunk)
    return 'sha256:' + h.hexdigest()
f = json.load(open(frozen_p)); bad = []
if sha(kettle) != f['executable']['digest']: bad.append(kettle)
now = {n: sha(os.path.join(sdir, n)) for n in sorted(os.listdir(sdir)) if os.path.isfile(os.path.join(sdir, n))}
if now != f['sidecar']['files']: bad.append(sdir)
if sha(weights) != f['model']['digest']: bad.append(weights)
for rel, want in f['implementation']['source_files'].items():
    if sha(rel) != want: bad.append(rel)
for sub in f['arms']:
    for rel, want in sub['pack_files'].items():
        local = 'inputs/%s/pack/%s' % (sub['id'], rel.split('packs/app.kttl.letter-to-actions/', 1)[1])
        if sha(local) != want: bad.append(local)
    m = sub.get('model')
    if m and sha(m['path']) != m['digest']: bad.append(m['path'])
for c in f['corpora']:
    if sha(c['path']) != c['digest']: bad.append(c['path'])
for rel, want in f['inventory_files'].items():
    if sha(rel) != want: bad.append(rel)
if sha('BUNDLE-MANIFEST.json') != f['bundle_manifest_digest']: bad.append('BUNDLE-MANIFEST.json')
if bad:
    print('refusing to run: these differ from the frozen plan:\n  ' + '\n  '.join(bad), file=sys.stderr); sys.exit(1)
print('  frozen pins hold: executable, %d sidecar files, weights, sources, packs, corpora, inventory' % len(now))
PY
}

# -------------------------------------------------------------------- run
# Pod. One arm: both selections, in the plan's order, under one shared
# monotonic deadline that starts before the first launch and is never
# extended. Each CLI runs in its own session/process group under the
# fixed environment; at the deadline the whole group (CLI and sidecar)
# gets TERM, then KILL. Partial output stays where it fell.
run() {
  local arm="${1:?run <arm>}"
  [[ -f "$FROZEN_REL" ]] || die "no $FROZEN_REL: run freeze first, then get it approved"
  plan_get "$FROZEN_REL" "any(a['id']=='$arm' for a in d['arms']) or (_ for _ in ()).throw(SystemExit('no arm called $arm in the frozen plan'))" >/dev/null
  verify_frozen
  local outroot; outroot="$(plan_get "$FROZEN_REL" "d['output_root']")"
  local armdir="$outroot/$arm"
  [[ ! -e "$armdir" ]] || die "$armdir exists; an arm is never rerun or replaced"
  local requires; requires="$(plan_get "$FROZEN_REL" "next(a for a in d['arms'] if a['id']=='$arm').get('requires') or ''")"
  if [[ -n "$requires" ]]; then
    local prev="$outroot/$requires/receipt.json"
    [[ -f "$prev" ]] || die "arm $requires has no receipt; the plan runs $requires before $arm"
    [[ "$(plan_get "$prev" "d['status']")" == completed ]] || die "arm $requires did not complete; the sitting stops here, no next arm"
  fi
  local weights; weights="$(plan_get "$FROZEN_REL" "(next(a for a in d['arms'] if a['id']=='$arm').get('model') or d['model'])['path']")"
  if pgrep -f llama-server >/dev/null; then die "a llama-server is already running; two chains look like one"; fi
  local sidecar_bin="$SIDECAR_DIR_REL/llama-server"
  local context; context="$(plan_get "$FROZEN_REL" "d['policy']['context']")"
  mkdir -p "$armdir/inputs" "target/eval-logs"
  cp -R "inputs/$arm/pack" "$armdir/inputs/pack"
  cp "$FROZEN_REL" "$armdir/measurement-02-pod.frozen.json"
  cp BUNDLE-MANIFEST.json "$armdir/"
  nvidia-smi --query-gpu=name,memory.used,utilization.gpu --format=csv > "$armdir/nvidia-smi-before.txt"

  local -a env_kv; read -r -a env_kv <<<"$(fixed_env)"
  # Job control makes every background job its own process group, so the
  # CLI's pgid is its pid and the sidecar it spawns is in that group.
  set -m
  local receipt="$armdir/receipt.json"
  local started_wall; started_wall="$(date -u +%FT%T.%NZ)"
  local start_mono; start_mono="$(mono)"
  local deadline=$(( start_mono + DEADLINE_SECONDS ))
  local status="running"
  python3 - "$receipt" "$arm" "$started_wall" "$DEADLINE_SECONDS" "${env_kv[@]}" <<'PY'
import json, sys
r = {'schema': 'kettle/corpus-measurement-receipt@2', 'arm': sys.argv[2], 'status': 'running',
     'started_at': sys.argv[3], 'deadline_seconds': int(sys.argv[4]), 'environment': sys.argv[5:],
     'selections': []}
json.dump(r, open(sys.argv[1], 'w'), indent=2)
PY

  local sel timed_out=0
  for sel in diagnostic-01 product-regressions-01; do
    local out="$armdir/$sel"
    local remaining=$(( deadline - $(mono) ))
    if (( remaining <= 0 )); then
      note "$arm/$sel not launched: the arm's deadline has passed"
      python3 - "$receipt" "$sel" <<'PY'
import json, sys
r = json.load(open(sys.argv[1])); r['selections'].append({'selection': sys.argv[2], 'status': 'not launched: deadline passed'}); json.dump(r, open(sys.argv[1], 'w'), indent=2)
PY
      timed_out=1; continue
    fi
    note "$arm/$sel: launching with $remaining s left on the arm's deadline"
    local before_logs; before_logs="$(ls target/eval-logs 2>/dev/null || true)"
    local sel_started; sel_started="$(date -u +%FT%T.%NZ)"
    local sel_mono0; sel_mono0="$(mono)"
    local -a cmdv=("$root/$KETTLE_REL" corpus
      --corpus "$root/evals/corpus/$sel.json" --inventory-dir "$root/evals/capabilities"
      --pack-dir "$root/inputs/$arm/pack" --model "$root/$weights"
      --sidecar-binary "$root/$sidecar_bin" --sidecars-dir "$root/sidecars"
      --context "$context" --out "$root/$out")
    env -i "${env_kv[@]}" "${cmdv[@]}" >"$armdir/$sel.stdout" 2>"$armdir/$sel.stderr" &
    local pid=$!
    sleep 1
    local pgid; pgid="$(ps -o pgid= -p "$pid" 2>/dev/null | tr -d ' ' || true)"
    [[ "$pgid" == "$pid" ]] || note "warning: pgid $pgid differs from pid $pid; the group kill targets $pgid"
    [[ -n "$pgid" ]] || pgid="$pid"
    local sel_timeout=0 sampled=0
    while kill -0 "$pid" 2>/dev/null; do
      if (( $(mono) >= deadline )); then
        note "deadline reached: terminating process group $pgid (CLI and sidecar)"
        kill -TERM -- "-$pgid" 2>/dev/null || kill -TERM "$pid" 2>/dev/null || true
        sleep 10
        kill -KILL -- "-$pgid" 2>/dev/null || true
        sel_timeout=1; timed_out=1; break
      fi
      # Two GPU readings during the run, a minute apart: 0% during load means nothing.
      if (( sampled < 2 )) && (( $(mono) - sel_mono0 >= 30 + sampled * 60 )); then
        nvidia-smi --query-gpu=memory.used,utilization.gpu --format=csv,noheader >> "$armdir/nvidia-smi-during-$sel.txt" || true
        sampled=$((sampled + 1))
      fi
      sleep 1
    done
    local exit_code=0
    wait "$pid" || exit_code=$?
    local sel_finished; sel_finished="$(date -u +%FT%T.%NZ)"
    local elapsed=$(( $(mono) - sel_mono0 ))
    # The sidecar log is the proof reasoning was off and the model was on the card.
    local new_log; new_log="$(comm -13 <(echo "$before_logs" | sort) <(ls target/eval-logs | sort) | grep '^corpus-' | head -1 || true)"
    local thinking="unknown" device="unknown"
    if [[ -n "$new_log" ]]; then
      cp "target/eval-logs/$new_log" "$armdir/sidecar-$sel.log"
      thinking="$(grep -o 'thinking = [0-9]*' "$armdir/sidecar-$sel.log" | head -1 || echo unknown)"
      device="$(grep -o 'using device [^(]*([^)]*)' "$armdir/sidecar-$sel.log" | head -1 || echo unknown)"
    fi
    python3 - "$receipt" "$sel" "$sel_started" "$sel_finished" "$elapsed" "$exit_code" "$sel_timeout" "$thinking" "$device" "${cmdv[@]}" <<'PY'
import json, sys
r = json.load(open(sys.argv[1]))
sel, started, finished, elapsed, code, to, thinking, device = sys.argv[2:10]
r['selections'].append({'selection': sel, 'status': 'timed out' if to == '1' else ('completed' if code == '0' else 'failed'),
    'exit_code': int(code), 'started_at': started, 'finished_at': finished, 'elapsed_seconds': int(elapsed),
    'sidecar_thinking': thinking, 'sidecar_device': device, 'command': sys.argv[10:]})
json.dump(r, open(sys.argv[1], 'w'), indent=2)
PY
    note "$arm/$sel: exit $exit_code after ${elapsed}s; sidecar says '$thinking', '$device'"
    if (( sel_timeout )); then break; fi
    if (( exit_code != 0 )); then
      note "$arm/$sel failed (exit $exit_code); the sitting stops here with what was retained"
      status="failed"; break
    fi
  done
  nvidia-smi --query-gpu=name,memory.used,utilization.gpu --format=csv > "$armdir/nvidia-smi-after.txt" || true
  if (( timed_out )); then status="timed out"; elif [[ "$status" == running ]]; then status="completed"; fi
  python3 - "$receipt" "$status" "$(date -u +%FT%T.%NZ)" "$(( $(mono) - start_mono ))" <<'PY'
import json, sys
r = json.load(open(sys.argv[1])); r['status'] = sys.argv[2]; r['finished_at'] = sys.argv[3]; r['elapsed_seconds'] = int(sys.argv[4])
json.dump(r, open(sys.argv[1], 'w'), indent=2)
PY
  exchange_summary "$armdir"
  echo "run $arm: $status (receipt $receipt)"
  [[ "$status" == completed ]] || exit 1
}

# Count every exchange from the recordings, and say which kind each one
# is by the marker the executor itself appends: a schema retry quotes
# the previous answer, a pairing re-ask carries REJOIN_NOTE, and any
# other later exchange on the same case and step is a truncation split.
exchange_summary() {
  local armdir="$1"
  python3 - "$armdir" <<'PY'
import glob, json, os, re, sys
armdir = sys.argv[1]
SCHEMA = 'It did not match what was asked for:'
REJOIN = 'A previous answer dropped some of these items or paired them'
out = {'selections': {}, 'totals': {'initial': 0, 'schema_retry': 0, 'pairing_reask': 0, 'truncation_split': 0, 'all': 0}}
for sel in ('diagnostic-01', 'product-regressions-01'):
    rp = os.path.join(armdir, sel, 'report.json')
    entry = {'cases': {}, 'initial': 0, 'schema_retry': 0, 'pairing_reask': 0, 'truncation_split': 0, 'all': 0,
             'generation_files': len(glob.glob(os.path.join(armdir, sel, 'case-*', 'raw', '*.generation.json')))}
    if os.path.exists(rp):
        r = json.load(open(rp))
        entry['cases_reported'] = len(r['cases']); entry['unscored_cases'] = r.get('unscored_cases')
        for c in r['cases']:
            asked = {}; kinds = []
            for e in c.get('exchanges', []):
                req = e.get('request', ''); step = e.get('step')
                # Item ids the executor rendered; the worked example's ids live in the 900s and are never asked.
                ids = {int(i) for i in re.findall(r'"id":\s*(\d+)', req) if int(i) < 899}
                if SCHEMA in req: k = 'schema_retry'
                elif REJOIN in req: k = 'pairing_reask'
                elif ids and any(ids < prev for prev in asked.get(step, [])): k = 'truncation_split'
                else: k = 'initial'
                asked.setdefault(step, []).append(ids); kinds.append(k); entry[k] += 1
            entry['all'] += len(kinds)
            if len(kinds) != 1: entry['cases'][c['case']] = kinds
    else:
        entry['report'] = 'absent (incomplete selection)'
    out['selections'][sel] = entry
    for k in out['totals']: out['totals'][k] += entry.get(k, 0)
json.dump(out, open(os.path.join(armdir, 'exchanges.json'), 'w'), indent=2)
t = out['totals']
print('  exchanges: %d total = %d initial + %d schema retries + %d pairing re-asks + %d truncation splits' % (t['all'], t['initial'], t['schema_retry'], t['pairing_reask'], t['truncation_split']))
PY
}

# ----------------------------------------------------------------- replay
# Exact replay of each selection with the arm's own pack into new
# directories, then a comparison that requires identical per-case
# scores, identical summaries, one exact request per recorded exchange
# and zero legacy matches.
replay() {
  local arm="${1:?replay <arm>}"
  [[ -f "$FROZEN_REL" ]] || die "no frozen plan"
  local outroot; outroot="$(plan_get "$FROZEN_REL" "d['output_root']")"
  local armdir="$outroot/$arm"
  [[ -f "$armdir/receipt.json" ]] || die "no receipt for arm $arm"
  local -a env_kv; read -r -a env_kv <<<"$(fixed_env)"
  local sel
  for sel in diagnostic-01 product-regressions-01; do
    [[ -f "$armdir/$sel/report.json" ]] || { note "$arm/$sel has no report; nothing to replay"; continue; }
    [[ ! -e "$armdir/$sel-replay" ]] || die "$armdir/$sel-replay exists"
    note "replaying $arm/$sel with its own pack"
    env -i "${env_kv[@]}" "$root/$KETTLE_REL" corpus \
      --corpus "$root/evals/corpus/$sel.json" --inventory-dir "$root/evals/capabilities" \
      --pack-dir "$root/inputs/$arm/pack" --replay "$root/$armdir/$sel" \
      --out "$root/$armdir/$sel-replay" >"$armdir/$sel-replay.stdout" 2>"$armdir/$sel-replay.stderr" \
      || { tail -5 "$armdir/$sel-replay.stderr" >&2; die "replay failed for $arm/$sel (kept)"; }
  done
  replay_check "$armdir"
  echo "replay $arm: exact"
}

replay_check() {
  local armdir="${1:?replay-check <arm-dir>}"
  python3 - "$armdir" <<'PY'
import json, os, sys
armdir = sys.argv[1]; result = {}; ok = True
for sel in ('diagnostic-01', 'product-regressions-01'):
    a = os.path.join(armdir, sel, 'report.json'); b = os.path.join(armdir, sel + '-replay', 'report.json')
    if not (os.path.exists(a) and os.path.exists(b)): result[sel] = 'absent'; continue
    A = json.load(open(a)); B = json.load(open(b))
    diffs = []
    for ca, cb in zip(A['cases'], B['cases']):
        for k in ('score', 'raw', 'verified', 'acquisition_errors', 'attribution_errors', 'execution_error', 'coverage'):
            if ca.get(k) != cb.get(k): diffs.append('%s.%s' % (ca['case'], k))
    if len(A['cases']) != len(B['cases']): diffs.append('case count')
    if A['summary'] != B['summary']: diffs.append('summary')
    for k in ('model', 'sidecar', 'runtime', 'generation_machine', 'corpus_digest', 'pipeline_digest', 'scoring', 'selection'):
        if A.get(k) != B.get(k): diffs.append('identity.' + k)
    exchanges = sum(len(c.get('exchanges', [])) for c in A['cases'])
    rp = B.get('replay') or {}
    if rp.get('exact_requests') != exchanges: diffs.append('exact_requests %s != %d exchanges' % (rp.get('exact_requests'), exchanges))
    if rp.get('legacy_prompt_only_requests', 0) != 0: diffs.append('legacy matches %s' % rp.get('legacy_prompt_only_requests'))
    result[sel] = {'exchanges': exchanges, 'replay': rp, 'differences': diffs}
    ok = ok and not diffs
json.dump(result, open(os.path.join(armdir, 'replay-check.json'), 'w'), indent=2)
for sel, r in result.items(): print('  %s: %s' % (sel, r if r == 'absent' else ('exact, %d requests' % r['exchanges'] if not r['differences'] else 'DIFFERS: ' + ', '.join(r['differences']))))
sys.exit(0 if ok else 1)
PY
}

# ------------------------------------------------------------------- pack
# Pod. Everything the sitting produced, hashed, for the trip home. No
# weights, no sidecar, no build tree.
pack() {
  [[ -f "$FROZEN_REL" ]] || die "no frozen plan"
  local outroot; outroot="$(plan_get "$FROZEN_REL" "d['output_root']")"
  local id; id="$(plan_get "$FROZEN_REL" "d['id']")"
  local tgz="pod-corpus-$id-$(date -u +%Y%m%dT%H%M%SZ).tgz"
  (cd "$outroot" && find . -type f ! -name SHA256SUMS -exec sh -c 'sha256sum "$1"' _ {} \; | sort -k2 > SHA256SUMS)
  tar czf "$tgz" "$outroot" "$FROZEN_REL" BUNDLE-MANIFEST.json setup/*.log 2>/dev/null || tar czf "$tgz" "$outroot" "$FROZEN_REL" BUNDLE-MANIFEST.json
  echo "packed: $tgz ($(du -h "$tgz" | cut -f1)) sha256 $(sha256_of "$tgz")"
  echo "$(tar tzf "$tgz" | wc -l) entries. Bring it home over the exposed TCP port (runpodctl pod get <id> → .ssh.ip/.ssh.port) or runpodctl send; verify the digest before destroying the pod."
}

case "$cmd" in
  bundle) bundle "$@" ;;
  check-bundle) check_bundle "$@" ;;
  no-model-checks) no_model_checks "$@" ;;
  preflight) preflight ;;
  setup) setup ;;
  freeze) freeze ;;
  run) run "$@" ;;
  replay) replay "$@" ;;
  pack) pack ;;
  exchange-summary) exchange_summary "$@" ;;
  replay-check) replay_check "$@" ;;
  *) die "unknown command '$cmd'" ;;
esac
